//! Request execution, SSE stream processing, and assistant finalization.
//!
//! Pure JSON request construction remains in `crate::request`; this module owns
//! the async network request loop, SSE accumulation, kernel/UI emission, and
//! assistant-history finalization for `OpenAiLlmClient`.
//!
//! Refs: docs/SPECS.md §Book III-B, I-Core-ChunkBudget, I-Shell-Network-Signal

use brioche_core::{ChatMessage, ToolCallDescriptor};
use brioche_shell_runtime::{BriocheShell, LlmChunk, ShellError, SystemSignal};
use bytes::Bytes;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::time::Duration;

use crate::client::{OpenAiError, OpenAiLlmClient};
use crate::sse::SseParser;

/// Internal accumulator for an in-flight SSE tool call.
///
/// Refs: I-Shell-Network-Signal
#[derive(Clone, Debug, Default)]
pub(super) struct ToolCallAccumulator {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) args: String,
    pub(super) start_emitted: bool,
}

// ---------------------------------------------------------------------------
// Request building — pure, no I/O.
// ---------------------------------------------------------------------------

/// Mutable accumulator state passed through the SSE event loop.
///
/// Kept in one struct so `process_sse_event` stays under the
/// clippy argument-count limit.
struct SseAccumulators {
    first_chunk: bool,
    content_chars: usize,
    reasoning_chars: usize,
    tool_acc: BTreeMap<usize, ToolCallAccumulator>,
    finish_reason: Option<String>,
}

/// Accumulated outcome of a complete SSE stream.
///
/// The stream processor emits chunks in real time (to the kernel
/// and the UI) and returns this struct so the client can decide
/// how to persist the assistant message.
///
/// Refs: I-Core-ChunkBudget, I-Shell-Network-Signal
pub(super) struct StreamOutcome {
    pub(super) finish_reason: Option<String>,
    pub(super) tool_calls: BTreeMap<usize, ToolCallAccumulator>,
    pub(super) content_chars: usize,
    pub(super) reasoning_chars: usize,
}

impl OpenAiLlmClient {
    /// Process a single parsed SSE event.
    ///
    /// Emits text, reasoning, and tool-call fragments to the kernel
    /// and the UI in real time. Updates accumulators in place.
    ///
    /// # Complexity
    /// O(k) where k = number of choices in the event (typically 1).
    async fn process_sse_event(
        &self,
        shell: &BriocheShell,
        event: &serde_json::Value,
        acc: &mut SseAccumulators,
    ) -> Result<(), ShellError> {
        if let Some(err_msg) = self.error_detector.detect_error(event) {
            tracing::error!(%err_msg, "SSE provider error");
            let _ = self.ui_tx.send(LlmChunk::Error(err_msg.clone()));
            shell
                .send_system_signal(SystemSignal::NetworkUnavailable { reason: err_msg })
                .await?;
            return Ok(());
        }

        let Some(choices) = event.get("choices").and_then(|c| c.as_array()) else {
            if event.get("usage").is_some() {
                tracing::debug!(usage = ?event.get("usage"), "usage event");
            }
            return Ok(());
        };

        for choice in choices {
            let delta = choice.get("delta");
            let finish = choice
                .get("finish_reason")
                .and_then(|f| f.as_str())
                .map(|s| s.to_string());
            if finish.is_some() {
                acc.finish_reason = finish.clone();
                tracing::info!(finish_reason = ?finish, "finish_reason seen");
            }

            if let Some(extracted) = delta.and_then(|d| self.chunk_extractor.extract_text(d)) {
                if !acc.first_chunk {
                    acc.first_chunk = true;
                    let _ = self
                        .ui_tx
                        .send(LlmChunk::Status("Receiving response…".into()));
                }
                if extracted.is_reasoning {
                    acc.reasoning_chars += extracted.text.chars().count();
                    self.broadcast_reasoning(&extracted.text).await;
                } else {
                    acc.content_chars += extracted.text.chars().count();
                    self.emit_text_chunk(shell, &extracted.text).await?;
                }
            }

            if let Some(tool_calls) = delta
                .and_then(|d| d.get("tool_calls"))
                .and_then(|t| t.as_array())
            {
                if !acc.first_chunk {
                    acc.first_chunk = true;
                    let _ = self
                        .ui_tx
                        .send(LlmChunk::Status("Receiving response…".into()));
                }
                for tc in tool_calls {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).map_or(0, |v| v) as usize;
                    let entry = acc.tool_acc.entry(idx).or_default();

                    if let Some(id) = tc.get("id").and_then(|i| i.as_str())
                        && !id.is_empty()
                    {
                        entry.id = id.to_string();
                    }
                    let mut arg_fragment = String::new();
                    if let Some(func) = tc.get("function") {
                        if let Some(name) = func.get("name").and_then(|n| n.as_str())
                            && !name.is_empty()
                        {
                            entry.name = name.to_string();
                        }
                        if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                            entry.args.push_str(args);
                            arg_fragment = args.to_string();
                        }
                    }

                    if !entry.id.is_empty() && !entry.name.is_empty() && !entry.start_emitted {
                        self.emit_tool_call_start(shell, &entry.id, &entry.name)
                            .await?;
                        entry.start_emitted = true;
                    }

                    if entry.start_emitted && !arg_fragment.is_empty() {
                        self.emit_tool_argument(shell, &entry.id, &arg_fragment)
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Read the SSE byte stream until completion or timeout.
    ///
    /// Delegates per-event processing to `process_sse_event`. Owns
    /// the idle-timeout and heartbeat machinery.
    ///
    /// # Complexity
    /// O(m) where m = total SSE events. One allocation per chunk.
    pub(super) async fn read_sse_stream(
        &self,
        shell: &BriocheShell,
        mut stream: impl futures_util::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
        turn: usize,
    ) -> Result<StreamOutcome, ShellError> {
        let mut parser = SseParser::new();
        let mut total_bytes = 0usize;
        let mut chunk_count = 0usize;
        let mut event_count = 0usize;
        let content_chars = 0usize;
        let reasoning_chars = 0usize;

        let mut acc = SseAccumulators {
            first_chunk: false,
            content_chars: 0,
            reasoning_chars: 0,
            tool_acc: BTreeMap::new(),
            finish_reason: None,
        };

        const READ_TIMEOUT: Duration = Duration::from_secs(45);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            let chunk_result = match tokio::time::timeout(READ_TIMEOUT, stream.next()).await {
                Ok(Some(r)) => r,
                Ok(None) => {
                    let remaining = parser.remaining_buffer();
                    if !remaining.is_empty() {
                        let preview: String = remaining.chars().take(200).collect();
                        tracing::warn!(
                            preview = %preview,
                            len = remaining.len(),
                            "SSE stream closed with unprocessed data in parser buffer"
                        );
                    }
                    let _ = self.ui_tx.send(LlmChunk::Status(format!(
                        "Stream closed after {chunk_count} chunks, {total_bytes} bytes, {event_count} events"
                    )));
                    break;
                }
                Err(_) => {
                    let diag = format!(
                        "Idle timeout: no data for 45s. \
                        Chunks: {chunk_count}, bytes: {total_bytes}, events: {event_count}, \
                        content: {content_chars}c, reasoning: {reasoning_chars}c, \
                        tool_calls: {}, finish_reason: {:?}",
                        acc.tool_acc.len(),
                        acc.finish_reason
                    );
                    let _ = self.ui_tx.send(LlmChunk::Warning(diag.clone()));
                    shell
                        .send_system_signal(SystemSignal::NetworkUnavailable { reason: diag })
                        .await?;
                    return Err(OpenAiError::IdleTimeout.into());
                }
            };

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(err) => {
                    if acc.finish_reason.is_some() {
                        tracing::warn!(
                            error = %err,
                            finish_reason = ?acc.finish_reason,
                            "SSE stream error after finish_reason — using accumulated data"
                        );
                        let _ = self.ui_tx.send(LlmChunk::Warning(format!(
                            "Provider closed connection early ({err}). \
                            Using {event_count} events received so far."
                        )));
                        let remaining = parser.remaining_buffer();
                        if !remaining.is_empty() {
                            let preview: String = remaining.chars().take(200).collect();
                            tracing::warn!(
                                preview = %preview,
                                len = remaining.len(),
                                "SSE stream error with unprocessed data in parser buffer"
                            );
                        }
                        break;
                    }
                    let msg = format!("SSE error: {err}");
                    tracing::error!(error = %err, "SSE stream error");
                    let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                    shell
                        .send_system_signal(SystemSignal::NetworkUnavailable {
                            reason: msg.clone(),
                        })
                        .await?;
                    return Err(OpenAiError::Sse(msg).into());
                }
            };

            chunk_count += 1;
            total_bytes += chunk.len();

            tracing::trace!(
                turn = turn,
                chunk = chunk_count,
                bytes = chunk.len(),
                preview = %String::from_utf8_lossy(&chunk).chars().take(200).collect::<String>(),
                "SSE chunk"
            );

            if heartbeat.tick().await >= tokio::time::Instant::now() {
                let _ = self.ui_tx.send(LlmChunk::Status(format!(
                    "Still reading… {chunk_count} chunks, {total_bytes} bytes, {event_count} events"
                )));
            }

            let events = match parser.feed(&chunk) {
                Ok(events) => events,
                Err(err) => {
                    let msg = format!("SSE parser error: {err}");
                    tracing::error!(%msg, "SSE parser aborted after repeated malformed lines");
                    let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                    shell
                        .send_system_signal(SystemSignal::NetworkUnavailable {
                            reason: msg.clone(),
                        })
                        .await?;
                    return Err(OpenAiError::Sse(msg).into());
                }
            };
            for event in events {
                event_count += 1;
                tracing::debug!(event = %event.to_string(), "SSE event");
                self.process_sse_event(shell, &event, &mut acc).await?;
            }
        }

        Ok(StreamOutcome {
            finish_reason: acc.finish_reason,
            tool_calls: acc.tool_acc,
            content_chars: acc.content_chars,
            reasoning_chars: acc.reasoning_chars,
        })
    }

    /// Drain pending buffers and push the assistant message to history.
    ///
    /// # Complexity
    /// O(t) where t = number of tool calls.
    pub(super) async fn finalize_assistant_message(
        &self,
        outcome: StreamOutcome,
    ) -> Result<(), ShellError> {
        let text = {
            let mut pending = self.pending_text.lock().await;
            if !pending.is_empty() {
                Some(std::mem::take(&mut *pending))
            } else {
                None
            }
        };
        let reasoning = {
            let mut pending_reasoning = self.pending_reasoning_text.lock().await;
            if !pending_reasoning.is_empty() {
                Some(std::mem::take(&mut *pending_reasoning))
            } else {
                None
            }
        };

        let _ = self.ui_tx.send(LlmChunk::Status(format!(
            "Stream summary: content={}c, reasoning={}c, tool_calls={}, finish={:?}",
            outcome.content_chars,
            outcome.reasoning_chars,
            outcome.tool_calls.len(),
            outcome.finish_reason
        )));

        if outcome.finish_reason.as_deref() == Some("tool_calls") {
            let tool_calls: Vec<ToolCallDescriptor> = outcome
                .tool_calls
                .values()
                .map(|entry| ToolCallDescriptor {
                    tool_id: entry.id.clone(),
                    tool_name: entry.name.clone(),
                    arguments: entry.args.clone(),
                    timeout_ms: None,
                })
                .collect();

            let (sanitized, invalid) = Self::validate_and_sanitize_tool_calls(tool_calls);
            if !invalid.is_empty() {
                let diag = format!(
                    "Tool call arguments invalid JSON — {} failed:\n{}",
                    invalid.len(),
                    invalid.join("\n")
                );
                tracing::error!(%diag, "tool_call validation failed");
                let _ = self.ui_tx.send(LlmChunk::Warning(diag));
            }

            let content = match text {
                Some(t) => Self::truncate_assistant_text(t),
                None => String::new(),
            };
            self.history.write().await.push(ChatMessage::Assistant {
                content,
                reasoning,
                tool_calls: sanitized,
            });
        } else if let Some(text) = text {
            let trimmed = Self::truncate_assistant_text(text);
            self.history.write().await.push(ChatMessage::Assistant {
                content: trimmed,
                reasoning,
                tool_calls: Vec::new(),
            });
        } else if outcome.finish_reason.is_some() {
            let _ = self.ui_tx.send(LlmChunk::Warning(format!(
                "Model returned empty content (finish_reason={:?}). This may indicate a provider limitation or context window issue.",
                outcome.finish_reason
            )));
        } else {
            let _ = self.ui_tx.send(LlmChunk::Warning(
                "Stream closed with no content and no finish_reason. Provider may have dropped the connection.".into()
            ));
        }

        Ok(())
    }

    /// Validate JSON in tool-call arguments and replace invalid ones.
    ///
    /// Returns `(sanitized_calls, invalid_diagnostics)`.
    ///
    /// # Complexity
    /// O(t * a) where t = tool calls, a = average argument length.
    fn validate_and_sanitize_tool_calls(
        tool_calls: Vec<ToolCallDescriptor>,
    ) -> (Vec<ToolCallDescriptor>, Vec<String>) {
        let mut invalid = Vec::new();
        for (idx, tc) in tool_calls.iter().enumerate() {
            if !tc.arguments.is_empty()
                && serde_json::from_str::<serde_json::Value>(&tc.arguments).is_err()
            {
                invalid.push(format!(
                    "tool_call[{idx}] '{}' (id={}): args_preview={}",
                    tc.tool_name,
                    tc.tool_id,
                    tc.arguments.chars().take(80).collect::<String>()
                ));
            }
        }

        let sanitized = tool_calls
            .into_iter()
            .map(|mut tc| {
                if !tc.arguments.is_empty()
                    && serde_json::from_str::<serde_json::Value>(&tc.arguments).is_err()
                {
                    tc.arguments = r#"{"error":"truncated by provider"}"#.into();
                }
                tc
            })
            .collect();

        (sanitized, invalid)
    }

    /// Truncate assistant text to the hard history limit.
    ///
    /// # Complexity
    /// O(c) where c = char count.
    fn truncate_assistant_text(text: String) -> String {
        const MAX_ASSISTANT_CHARS: usize = 8000;
        if text.chars().count() > MAX_ASSISTANT_CHARS {
            let mut t: String = text.chars().take(MAX_ASSISTANT_CHARS - 3).collect();
            t.push_str("...");
            t
        } else {
            text
        }
    }
}
