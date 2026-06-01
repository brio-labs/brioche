//! `LlmClient` implementation for OpenAI-compatible endpoints.
//!
//! The client:
//! 1. Builds the JSON request from Brioche history.
//! 2. Opens an SSE connection via `reqwest`.
//! 3. Parses each SSE line into `delta.content` or `tool_calls`.
//! 4. Segments fragments according to `MAX_INLINE_CHUNK`.
//! 5. Sends each fragment to the kernel via `shell.send_input(LlmStream(...))`.
//! 6. Broadcasts chunks simultaneously on a `broadcast::Sender<LlmChunk>`
//!    channel so the projection (CLI) can display them in real time.
//!
//! # Invariants
//! - I-Core-ChunkBudget: any fragment > 4096 bytes is segmented.
//! - I-Shell-Network-Signal: on error, `SystemSignal::NetworkUnavailable`
//!   is emitted via the shell.
//!
//! Refs: SPECS.md §Book III-A, I-Core-ChunkBudget

use crate::{config::OpenAiConfig, request::build_request_body, sse::SseParser};
use brioche_core::{ChatMessage, MAX_INLINE_CHUNK, StreamEvent, ToolOutcome, ToolResultDTO};
use brioche_shell_runtime::{
    BriocheShell, EngineInput, LlmChunk, LlmClient, ShellError, SystemSignal,
};
use bytes::Bytes;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// OpenAI-compatible LLM client.
///
/// `tools_schema` is updated dynamically by the assembler (CLI)
/// when the tool registry changes. `Arc<RwLock>` allows updating
/// without reconstructing the client.
///
/// # Usage
/// ```ignore
/// let (client, llm_rx) = OpenAiLlmClient::new(config);
/// client.set_tools_schema(schemas).await;
/// // client is injected into DefaultEffectExecutor.
/// ```
/// Shared history mirror between the CLI and the LLM client.
///
/// The CLI pushes `UserMessage`s; the client pushes `Assistant`
/// and `ToolResult` messages as the stream progresses.
pub type SharedHistory = Arc<RwLock<Vec<ChatMessage>>>;

pub struct OpenAiLlmClient {
    config: OpenAiConfig,
    http: reqwest::Client,
    tools_schema: Arc<RwLock<Vec<serde_json::Value>>>,
    ui_tx: broadcast::Sender<LlmChunk>,
    /// Miroir de l'historique conversationnel.
    history: SharedHistory,
    /// Buffer local pour accumuler le texte assistant du stream courant.
    pending_text: tokio::sync::Mutex<String>,
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
        }
    }
}

impl OpenAiLlmClient {
    /// Creates a new client and returns the broadcast receiver.
    ///
    /// The broadcast channel has a capacity of 256 messages. Slow
    /// receivers may drop old messages.
    ///
    /// # Panics
    /// Never panics. Empty `api_key` is accepted (some local endpoints
    /// like Ollama do not require a key).
    pub fn new(config: OpenAiConfig) -> (Self, broadcast::Receiver<LlmChunk>, SharedHistory) {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let (ui_tx, ui_rx) = broadcast::channel(256);
        let history: SharedHistory = Arc::new(RwLock::new(Vec::new()));

        let client = Self {
            config,
            http,
            tools_schema: Arc::new(RwLock::new(Vec::new())),
            ui_tx,
            history: Arc::clone(&history),
            pending_text: tokio::sync::Mutex::new(String::new()),
        };

        (client, ui_rx, history)
    }

    /// Subscribes to the LLM chunk broadcast channel.
    ///
    /// Each call returns a new independent receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<LlmChunk> {
        self.ui_tx.subscribe()
    }

    /// Pushes a message into the history mirror.
    ///
    /// The CLI calls this method before sending a `UserMessage`
    /// to the shell, ensuring the LLM client sees the full history.
    pub async fn push_message(&self, message: ChatMessage) {
        self.history.write().await.push(message);
    }

    /// Updates the available tools list without rebuilding the client.
    ///
    /// This list is read at the start of each `call_llm()` call.
    pub async fn set_tools_schema(&self, schemas: Vec<serde_json::Value>) {
        let mut guard = self.tools_schema.write().await;
        *guard = schemas;
    }

    /// Segments a `Bytes` payload according to `MAX_INLINE_CHUNK`.
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

    /// Emits a text chunk to the kernel and the projection.
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

    /// Emits a tool call event to the kernel and the projection.
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

    /// Emits a tool call argument fragment.
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

    /// Emits the tool call end marker.
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

    /// Broadcasts a tool result to the projection (CLI).
    ///
    /// Called by `NotifyingToolExecutor` after execution.
    pub fn emit_tool_result(&self, name: &str, output: &str) {
        let _ = self.ui_tx.send(LlmChunk::ToolResult {
            name: name.to_string(),
            output: output.to_string(),
        });
    }

    /// Pushes tool results into the history mirror.
    ///
    /// The CLI (via an EffectExecutor wrapper) calls this method
    /// after executing tools, ensuring the next `call_llm()` call
    /// sees the results in the history.
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

/// Internal accumulator for a tool call being received over SSE.
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

        let history_guard = self.history.read().await;
        let messages = crate::request::build_messages(&history_guard);
        drop(history_guard);

        let tools_guard = self.tools_schema.read().await;
        let tools: Option<&[serde_json::Value]> = if tools_guard.is_empty() {
            None
        } else {
            Some(&*tools_guard)
        };

        let body = build_request_body(&self.config.model, messages, self.config.max_tokens, tools);
        drop(tools_guard);

        let request = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body);

        let response = match request.send().await {
            Ok(r) => r,
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
            // Extract a compact message from the OpenAI error JSON.
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
            tracing::error!(status = %status, body = %body_text, "OpenAI error response");
            let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
            shell
                .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                .await?;
            return Ok(());
        }

        let mut stream = response.bytes_stream();
        let mut parser = SseParser::new();

        // Accumulateur de tool calls par index OpenAI.
        let mut tool_acc: BTreeMap<usize, ToolCallAccumulator> = BTreeMap::new();
        let mut finish_reason: Option<String> = None;

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(err) => {
                    let msg = format!("SSE error: {err}");
                    tracing::error!(error = %err, "SSE stream error");
                    let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
                    shell
                        .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                        .await?;
                    return Ok(());
                }
            };

            for event in parser.feed(&chunk) {
                let Some(choices) = event.get("choices").and_then(|c| c.as_array()) else {
                    continue;
                };

                for choice in choices {
                    let delta = choice.get("delta");
                    let finish = choice
                        .get("finish_reason")
                        .and_then(|f| f.as_str())
                        .map(|s| s.to_string());
                    if finish.is_some() {
                        finish_reason = finish;
                    }

                    // Text chunk
                    if let Some(content) = delta
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                        && !content.is_empty()
                    {
                        self.emit_text_chunk(shell, content).await?;
                    }

                    // Tool calls (deltas partiels)
                    if let Some(tool_calls) = delta
                        .and_then(|d| d.get("tool_calls"))
                        .and_then(|t| t.as_array())
                    {
                        for (idx, tc) in tool_calls.iter().enumerate() {
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

                            // Emit ToolCallStart as soon as we have id + name.
                            if !entry.id.is_empty()
                                && !entry.name.is_empty()
                                && !entry.start_emitted
                            {
                                self.emit_tool_call_start(shell, &entry.id, &entry.name)
                                    .await?;
                                entry.start_emitted = true;
                            }

                            // Emit the argument fragment (only the delta,
                            // not the full accumulation — the kernel does push_str).
                            if entry.start_emitted && !arg_fragment.is_empty() {
                                self.emit_tool_argument(shell, &entry.id, &arg_fragment)
                                    .await?;
                            }
                        }
                    }
                }
            }
        }

        // If finish_reason is "tool_calls", all tool calls are complete.
        // Emit a single ToolCallDone (the kernel drains all pending).
        // Persist each complete ToolRequest with accumulated arguments.
        if finish_reason.as_deref() == Some("tool_calls")
            && let Some(first) = tool_acc.values().next()
        {
            self.emit_tool_call_done(shell, &first.id).await?;

            for entry in tool_acc.values() {
                self.history.write().await.push(ChatMessage::ToolRequest {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    arguments: entry.args.clone(),
                });
            }
        }

        // Mark the end of the stream and persist the assistant text.
        {
            let mut pending = self.pending_text.lock().await;
            let text = if !pending.is_empty() {
                Some(std::mem::take(&mut *pending))
            } else {
                None
            };
            drop(pending);
            if let Some(text) = text {
                self.history
                    .write()
                    .await
                    .push(ChatMessage::Assistant { content: text });
            }
        }

        shell
            .send_input(EngineInput::LlmStream(StreamEvent::Done))
            .await?;
        let _ = self.ui_tx.send(LlmChunk::Done);
        Ok(())
    }
}
