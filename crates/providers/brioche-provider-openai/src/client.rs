//! OpenAI-compatible LLM client implementation.
//!
//! The client:
//! 1. Builds the JSON request from Brioche history.
//! 2. Opens an SSE connection via `reqwest`.
//! 3. Parses each SSE line into `delta.content` or `tool_calls`.
//! 4. Segments fragments according to `MAX_INLINE_CHUNK`.
//! 5. Sends each fragment to the kernel via `shell.send_input(LlmStream(...))`.
//! 6. Broadcasts chunks on a `broadcast::Sender<LlmChunk>` channel
//!    so the projection (CLI) can display them in real time.
//!
//! # Invariants
//! - I-Core-ChunkBudget: any fragment > 4096 bytes is segmented.
//! - I-Shell-Network-Signal: on error, `SystemSignal::NetworkUnavailable`
//!   is emitted via the shell.
//!
//! Refs: SPECS.md §Book III-A, I-Core-ChunkBudget

use crate::{
    config::OpenAiConfig,
    extractor::{ChunkExtractor, StreamErrorDetector},
    request::build_request_body,
    sse::SseParser,
};
use brioche_core::{
    ChatMessage, MAX_INLINE_CHUNK, StreamEvent, ToolCallDescriptor, ToolOutcome, ToolResultDTO,
};
use brioche_shell_runtime::{
    BriocheShell, EngineInput, LlmChunk, LlmClient, ShellError, SystemSignal,
};
use bytes::Bytes;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast};

/// OpenAI-compatible LLM client.
///
/// `tools_schema` is updated dynamically by the assembler (CLI)
/// when the tool registry changes. `Arc<RwLock>` allows updates
/// without reconstructing the client.
///
/// # Usage
/// ```ignore
/// let (client, llm_rx) = OpenAiLlmClient::new(config);
/// client.set_tools_schema(schemas).await;
/// // client is injected into DefaultEffectExecutor.
/// ```
pub type SharedHistory = Arc<RwLock<Vec<ChatMessage>>>;

pub struct OpenAiLlmClient {
    config: OpenAiConfig,
    http: reqwest::Client,
    tools_schema: Arc<RwLock<Vec<serde_json::Value>>>,
    ui_tx: broadcast::Sender<LlmChunk>,
    history: SharedHistory,
    pending_text: tokio::sync::Mutex<String>,
    pending_reasoning_text: tokio::sync::Mutex<String>,
    chunk_extractor: Arc<dyn ChunkExtractor>,
    error_detector: Arc<dyn StreamErrorDetector>,
}

impl Clone for OpenAiLlmClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http: self.http.clone(),
            tools_schema: Arc::clone(&self.tools_schema),
            ui_tx: self.ui_tx.clone(),
            history: Arc::clone(&self.history),
            pending_text: tokio::sync::Mutex::new(String::new()),
            pending_reasoning_text: tokio::sync::Mutex::new(String::new()),
            chunk_extractor: Arc::clone(&self.chunk_extractor),
            error_detector: Arc::clone(&self.error_detector),
        }
    }
}

impl OpenAiLlmClient {
    /// Creates a new client and returns the broadcast receiver.
    ///
    /// The broadcast channel has capacity for 256 messages. Slow
    /// receivers may drop old messages.
    ///
    /// # Panics
    /// Never panics. Empty `api_key` is accepted (some local endpoints
    /// like Ollama do not require a key).
    pub fn new(config: OpenAiConfig) -> (Self, broadcast::Receiver<LlmChunk>, SharedHistory) {
        let http = reqwest::Client::builder()
            // No global request timeout — streaming generations can
            // take minutes (e.g. 80KB file writes). Idle detection
            // is handled by the per-chunk READ_TIMEOUT in call_llm().
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let (ui_tx, ui_rx) = broadcast::channel(256);
        let history: SharedHistory = Arc::new(RwLock::new(Vec::new()));
        let chunk_extractor = Arc::from(crate::extractor::chunk_extractor_for_model(&config.model));
        let error_detector = Arc::from(crate::extractor::error_detector_for_model(&config.model));

        let client = Self {
            config,
            http,
            tools_schema: Arc::new(RwLock::new(Vec::new())),
            ui_tx,
            history: Arc::clone(&history),
            pending_text: tokio::sync::Mutex::new(String::new()),
            pending_reasoning_text: tokio::sync::Mutex::new(String::new()),
            chunk_extractor,
            error_detector,
        };

        (client, ui_rx, history)
    }

    /// Subscribe to the LLM chunk broadcast channel.
    ///
    /// Each call returns an independent new receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<LlmChunk> {
        self.ui_tx.subscribe()
    }

    /// Push a message into the history mirror.
    ///
    /// The CLI calls this method before sending a `UserMessage`
    /// to the shell, ensuring the LLM client sees the complete history.
    pub async fn push_message(&self, message: ChatMessage) {
        self.history.write().await.push(message);
    }

    /// Update the available tools list without rebuilding the client.
    ///
    /// This list is read at the start of each `call_llm()` invocation.
    pub async fn set_tools_schema(&self, schemas: Vec<serde_json::Value>) {
        let mut guard = self.tools_schema.write().await;
        *guard = schemas;
    }

    /// Segment a `Bytes` payload according to `MAX_INLINE_CHUNK`.
    ///
    /// Refs: I-Core-ChunkBudget
    fn segment_bytes(bytes: Bytes, max_chunk: usize) -> Vec<Bytes> {
        if bytes.len() <= max_chunk {
            return vec![bytes];
        }
        let mut fragments = Vec::with_capacity(bytes.len().div_ceil(max_chunk));
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + max_chunk).min(bytes.len());
            fragments.push(bytes.slice(offset..end));
            offset = end;
        }
        fragments
    }

    /// Emit a text chunk to the kernel and the projection.
    async fn emit_text_chunk(&self, shell: &BriocheShell, text: &str) -> Result<(), ShellError> {
        {
            let mut pending = self.pending_text.lock().await;
            pending.push_str(text);
        }
        let bytes = Bytes::from(text.to_string());
        for chunk in Self::segment_bytes(bytes, MAX_INLINE_CHUNK) {
            shell
                .send_input(EngineInput::LlmStream(StreamEvent::TextChunk {
                    path: Default::default(),
                    chunk,
                }))
                .await?;
        }
        let _ = self.ui_tx.send(LlmChunk::Text(text.to_string()));
        Ok(())
    }

    /// Broadcasts reasoning text to the UI projection only.
    ///
    /// Reasoning text is displayed to the user for transparency
    /// but is **not** sent to the kernel and is **not** accumulated
    /// in `pending_text` (so it never pollutes the conversation
    /// history).
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    async fn broadcast_reasoning(&self, text: &str) {
        {
            let mut pending = self.pending_reasoning_text.lock().await;
            pending.push_str(text);
        }
        let _ = self.ui_tx.send(LlmChunk::Reasoning(text.to_string()));
    }

    /// Emit a tool call event to the kernel and the projection.
    async fn emit_tool_call_start(
        &self,
        shell: &BriocheShell,
        id: &str,
        name: &str,
    ) -> Result<(), ShellError> {
        shell
            .send_input(EngineInput::LlmStream(StreamEvent::ToolCallStart {
                path: Default::default(),
                id: id.to_string(),
                name: name.to_string(),
            }))
            .await?;
        let _ = self.ui_tx.send(LlmChunk::ToolCallStart {
            id: id.to_string(),
            name: name.to_string(),
        });
        Ok(())
    }

    /// Emit a tool call argument fragment.
    async fn emit_tool_argument(
        &self,
        shell: &BriocheShell,
        id: &str,
        fragment: &str,
    ) -> Result<(), ShellError> {
        let bytes = Bytes::from(fragment.to_string());
        for chunk in Self::segment_bytes(bytes, MAX_INLINE_CHUNK) {
            shell
                .send_input(EngineInput::LlmStream(StreamEvent::ToolArgumentChunk {
                    path: Default::default(),
                    id: id.to_string(),
                    chunk,
                }))
                .await?;
        }
        let _ = self.ui_tx.send(LlmChunk::ToolArgument {
            id: id.to_string(),
            fragment: fragment.to_string(),
        });
        Ok(())
    }

    /// Emit the tool call done marker.
    async fn emit_tool_call_done(&self, shell: &BriocheShell, id: &str) -> Result<(), ShellError> {
        shell
            .send_input(EngineInput::LlmStream(StreamEvent::ToolCallDone {
                path: Default::default(),
            }))
            .await?;
        let _ = self
            .ui_tx
            .send(LlmChunk::ToolCallDone { id: id.to_string() });
        Ok(())
    }

    /// Broadcast a tool result to the projection (CLI).
    ///
    /// Called by the effect executor after execution.
    pub fn emit_tool_result(&self, name: &str, output: &str) {
        let _ = self.ui_tx.send(LlmChunk::ToolResult {
            name: name.to_string(),
            output: output.to_string(),
        });
    }

    /// Push tool execution results into the history mirror.
    ///
    /// The effect executor calls this method after executing tools,
    /// ensuring the next `call_llm()` sees the results in history.
    pub async fn push_tool_results(&self, results: &[ToolResultDTO]) {
        let mut history = self.history.write().await;
        for result in results {
            let content = match &result.outcome {
                ToolOutcome::Success(s)
                | ToolOutcome::BusinessError(s)
                | ToolOutcome::SystemError(s) => s.clone(),
                ToolOutcome::TimeoutWithPartialData { partial_output } => {
                    partial_output.clone().unwrap_or_default()
                }
            };
            history.push(ChatMessage::ToolResult {
                id: result.tool_id.clone(),
                content,
            });
        }
    }
}

/// Internal accumulator for an in-flight SSE tool call.
#[derive(Clone, Debug, Default)]
struct ToolCallAccumulator {
    id: String,
    name: String,
    args: String,
    start_emitted: bool,
}

#[async_trait::async_trait]
impl LlmClient for OpenAiLlmClient {
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), ShellError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let turn = {
            let history = self.history.read().await;
            history
                .iter()
                .filter(|m| matches!(m, ChatMessage::Assistant { .. }))
                .count()
                + 1
        };
        let _ = self.ui_tx.send(LlmChunk::Status(format!(
            "Calling LLM (turn {turn})…"
        )));

        let history_guard = self.history.read().await;
        let messages = crate::request::build_messages(&history_guard);
        let msg_count = messages.len();
        drop(history_guard);

        let tools_guard = self.tools_schema.read().await;
        let tools: Option<&[serde_json::Value]> = if tools_guard.is_empty() {
            None
        } else {
            Some(&*tools_guard)
        };

        let body = build_request_body(
            &self.config.model,
            messages,
            self.config.max_tokens,
            self.config.reasoning_effort.as_deref(),
            tools,
        );
        drop(tools_guard);

        let body_str = body.to_string();

        // Diagnostic: write request body to temp file before sending.
        // Activated by BRIOCHE_DIAG=1 env var.
        if std::env::var("BRIOCHE_DIAG").is_ok() {
            let _ = std::fs::write(format!("/tmp/brioche_request_turn_{turn}.json"), &body_str);
        }

        let _ = self.ui_tx.send(LlmChunk::Status(format!(
            "HTTP POST {} — {} messages, ~{} chars",
            url, msg_count, body_str.len(),
        )));

        let request = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body);

        let response = match request.send().await {
            Ok(r) => {
                let _ = self.ui_tx.send(LlmChunk::Status(format!(
                    "HTTP {} — starting SSE stream",
                    r.status()
                )));
                r
            }
            Err(err) => {
                let msg = format!("Network error: {err}");
                tracing::error!(error = %err, "OpenAI request failed");
                let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                shell
                    .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                    .await?;
                return Ok(());
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let compact = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                json.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or(&body_text)
                    .to_string()
            } else {
                body_text.lines().next().unwrap_or(&body_text).to_string()
            };
            let msg = format!("HTTP {status}: {compact}");
            tracing::error!(status = %status, error = %body_text, "OpenAI HTTP error");
            let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
            shell
                .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                .await?;
            return Ok(());
        }

        let mut stream = response.bytes_stream();
        let mut parser = SseParser::new();
        let mut total_bytes = 0usize;
        let mut chunk_count = 0usize;
        let mut event_count = 0usize;
        let mut content_chars = 0usize;
        let mut reasoning_chars = 0usize;
        let mut last_activity = Instant::now();
        let stream_start = Instant::now();

        let mut tool_acc: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();
        let mut finish_reason: Option<String> = None;
        let mut first_chunk_seen = false;

        const READ_TIMEOUT: Duration = Duration::from_secs(45);
        const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
        let mut next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;

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
                        "Stream closed after {chunk_count} chunks, {total_bytes} bytes, {event_count} events ({:.1}s)",
                        stream_start.elapsed().as_secs_f64()
                    )));
                    break;
                }
                Err(_) => {
                    let idle_secs = last_activity.elapsed().as_secs();
                    let diag = format!(
                        "Idle timeout: no data for {idle_secs}s. \
                        Chunks: {chunk_count}, bytes: {total_bytes}, events: {event_count}, \
                        content: {content_chars}c, reasoning: {reasoning_chars}c, \
                        tool_calls: {}, finish_reason: {:?}",
                        tool_acc.len(),
                        finish_reason
                    );
                    tracing::warn!(%diag, "SSE read timeout");
                    let _ = self.ui_tx.send(LlmChunk::Warning(diag.clone()));
                    shell
                        .send_system_signal(SystemSignal::NetworkUnavailable { reason: diag })
                        .await?;
                    return Ok(());
                }
            };

            let chunk = match chunk_result {
                Ok(c) => c,
                Err(err) => {
                    if finish_reason.is_some() {
                        tracing::warn!(
                            error = %err,
                            finish_reason = ?finish_reason,
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
                        .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                        .await?;
                    return Ok(());
                }
            };

            chunk_count += 1;
            total_bytes += chunk.len();
            last_activity = Instant::now();

            tracing::trace!(
                turn = turn,
                chunk = chunk_count,
                bytes = chunk.len(),
                preview = %String::from_utf8_lossy(&chunk).chars().take(200).collect::<String>(),
                "SSE chunk"
            );

            if Instant::now() > next_heartbeat {
                let _ = self.ui_tx.send(LlmChunk::Status(format!(
                    "Still reading… {chunk_count} chunks, {total_bytes} bytes, {event_count} events ({:.1}s)",
                    stream_start.elapsed().as_secs_f64()
                )));
                next_heartbeat = Instant::now() + HEARTBEAT_INTERVAL;
            }

            for event in parser.feed(&chunk) {
                event_count += 1;
                tracing::debug!(event = %event.to_string(), "SSE event");

                if let Some(err_msg) = self.error_detector.detect_error(&event) {
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
                    continue;
                };

                for choice in choices {
                    let delta = choice.get("delta");
                    let finish = choice
                        .get("finish_reason")
                        .and_then(|f| f.as_str())
                        .map(|s| s.to_string());
                    if finish.is_some() {
                        finish_reason = finish.clone();
                        tracing::info!(finish_reason = ?finish, "finish_reason seen");
                    }

                    if let Some(extracted) =
                        delta.and_then(|d| self.chunk_extractor.extract_text(d))
                    {
                        if !first_chunk_seen {
                            first_chunk_seen = true;
                            let _ = self.ui_tx.send(LlmChunk::Status("Receiving response…".into()));
                        }
                        if extracted.is_reasoning {
                            reasoning_chars += extracted.text.chars().count();
                            self.broadcast_reasoning(&extracted.text).await;
                        } else {
                            content_chars += extracted.text.chars().count();
                            self.emit_text_chunk(shell, &extracted.text).await?;
                        }
                    }

                    if let Some(tool_calls) = delta
                        .and_then(|d| d.get("tool_calls"))
                        .and_then(|t| t.as_array())
                    {
                        if !first_chunk_seen {
                            first_chunk_seen = true;
                            let _ = self.ui_tx.send(LlmChunk::Status("Receiving response…".into()));
                        }
                        for tc in tool_calls {
                            let idx =
                                tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                            let entry = tool_acc.entry(idx).or_default();

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

                            if !entry.id.is_empty()
                                && !entry.name.is_empty()
                                && !entry.start_emitted
                            {
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
            }
        }

        if finish_reason.as_deref() == Some("tool_calls")
            && let Some(first) = tool_acc.values().next()
        {
            self.emit_tool_call_done(shell, &first.id).await?;
        }

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

        const MAX_ASSISTANT_CHARS: usize = 8000;
        let truncate = |text: String| -> String {
            if text.chars().count() > MAX_ASSISTANT_CHARS {
                let mut t: String = text.chars().take(MAX_ASSISTANT_CHARS - 3).collect();
                t.push_str("...");
                t
            } else {
                text
            }
        };

        let _ = self.ui_tx.send(LlmChunk::Status(format!(
            "Stream summary: content={content_chars}c, reasoning={reasoning_chars}c, tool_calls={}, finish={:?}",
            tool_acc.len(),
            finish_reason
        )));

        if finish_reason.as_deref() == Some("tool_calls") {
            let tool_calls: Vec<ToolCallDescriptor> = tool_acc
                .values()
                .map(|entry| ToolCallDescriptor {
                    tool_id: entry.id.clone(),
                    tool_name: entry.name.clone(),
                    arguments: entry.args.clone(),
                    timeout_ms: None,
                })
                .collect();

            let mut invalid = Vec::new();
            for (idx, tc) in tool_calls.iter().enumerate() {
                if !tc.arguments.is_empty()
                    && let Err(err) = serde_json::from_str::<serde_json::Value>(&tc.arguments)
                {
                    invalid.push(format!(
                        "tool_call[{idx}] '{}' (id={}): {err} | args_preview={}",
                        tc.tool_name,
                        tc.tool_id,
                        tc.arguments.chars().take(80).collect::<String>()
                    ));
                }
            }

            if !invalid.is_empty() {
                let diag = format!(
                    "Tool call arguments invalid JSON — {} of {} failed:\n{}",
                    invalid.len(),
                    tool_calls.len(),
                    invalid.join("\n")
                );
                tracing::error!(%diag, "tool_call validation failed");
                let _ = self.ui_tx.send(LlmChunk::Warning(diag));
            }

            let content = text.map(truncate).unwrap_or_default();

            let sanitized_tool_calls: Vec<ToolCallDescriptor> = tool_calls
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

            self.history.write().await.push(ChatMessage::Assistant {
                content,
                reasoning,
                tool_calls: sanitized_tool_calls,
            });
        } else if let Some(text) = text {
            let trimmed = truncate(text);
            self.history.write().await.push(ChatMessage::Assistant {
                content: trimmed,
                reasoning,
                tool_calls: Vec::new(),
            });
        } else if finish_reason.is_some() {
            let _ = self.ui_tx.send(LlmChunk::Warning(format!(
                "Model returned empty content (finish_reason={:?}). This may indicate a provider limitation or context window issue.",
                finish_reason
            )));
        } else {
            let _ = self.ui_tx.send(LlmChunk::Warning(
                "Stream closed with no content and no finish_reason. Provider may have dropped the connection.".into()
            ));
        }

        shell
            .send_input(EngineInput::LlmStream(StreamEvent::Done))
            .await?;
        let _ = self.ui_tx.send(LlmChunk::Done);
        Ok(())
    }

    async fn push_tool_results(&self, results: &[ToolResultDTO]) {
        OpenAiLlmClient::push_tool_results(self, results).await;
    }
}
