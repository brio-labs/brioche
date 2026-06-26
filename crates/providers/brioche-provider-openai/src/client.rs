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
//! Refs: docs/SPECS.md §Book III-A, I-Core-ChunkBudget

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use brioche_core::{
    ChatMessage, MAX_INLINE_CHUNK, StreamEvent, ToolCallDescriptor, ToolOutcome, ToolResultDTO,
};
use brioche_shell_runtime::{
    BriocheShell, EngineInput, LlmChunk, LlmClient, ShellError, SystemSignal,
};
use bytes::Bytes;
use futures_util::StreamExt;
use tokio::sync::{RwLock, broadcast};

use crate::config::OpenAiConfig;
use crate::extractor::{ChunkExtractor, StreamErrorDetector};
use crate::request::build_request_body;
use crate::sse::SseParser;

/// Provider-specific error returned by `OpenAiLlmClient` operations.
///
/// Preserves OpenAI-specific context (HTTP status, SSE diagnostics, parse
/// failures) and is converted to a generic [`ShellError`] at the trait
/// boundary.
///
/// Refs: docs/SPECS.md §Book III-B
#[derive(Debug, thiserror::Error)]
pub enum OpenAiError {
    /// The HTTP request could not be sent or the connection failed.
    #[error("network request failed: {0}")]
    Network(String),
    /// The provider returned a non-success HTTP status.
    #[error("HTTP {status}: {message}")]
    Http {
        /// HTTP status code returned by the provider.
        status: u16,
        /// Compacted error message extracted from the response body.
        message: String,
    },
    /// No SSE data was received within the configured idle timeout.
    #[error("SSE stream idle timeout")]
    IdleTimeout,
    /// The SSE stream failed or contained malformed data.
    #[error("SSE provider error: {0}")]
    Sse(String),
    /// The summary response could not be parsed as JSON.
    #[error("failed to parse summary response: {0}")]
    SummaryParse(String),
}

impl From<OpenAiError> for ShellError {
    fn from(err: OpenAiError) -> Self {
        ShellError::EffectExecution(err.to_string())
    }
}

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

/// Transform applied to the conversational history before building an LLM request.
///
/// The mirror history stays unchanged; only the request payload is affected.
/// This hook is used by desktop extensions such as context engines and memory
/// providers.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub type HistoryTransform = Arc<dyn Fn(&[ChatMessage]) -> Vec<ChatMessage> + Send + Sync>;

/// Maximum number of bytes to read from an HTTP error response body.
///
/// Prevents a malicious or misbehaving provider from OOM-ing the shell by
/// returning an unbounded error payload. The limit is applied while streaming
/// chunks, so no more than this amount is buffered.
///
/// Refs: docs/SPECS.md §Book III-B
pub const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// Reads at most [`MAX_ERROR_BODY_BYTES`] from `response`, then converts the
/// buffered bytes to a string, replacing invalid UTF-8 sequences.
///
/// Streaming stops as soon as the limit is reached; remaining bytes are
/// discarded. This function consumes the response body.
///
/// Refs: docs/SPECS.md §Book III-B
async fn limited_error_body(mut response: reqwest::Response, limit: usize) -> String {
    let mut collected = Vec::with_capacity(limit.min(4096));
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let remaining = limit.saturating_sub(collected.len());
                if remaining == 0 {
                    break;
                }
                let take = chunk.len().min(remaining);
                collected.extend_from_slice(&chunk[..take]);
                if collected.len() >= limit {
                    break;
                }
            }
            Ok(None) => break,
            Err(err) => {
                tracing::debug!(error = %err, "failed to read error response chunk");
                break;
            }
        }
    }
    String::from_utf8_lossy(&collected).into_owned()
}

/// OpenAI-compatible LLM client implementation.
///
/// Handles SSE streaming, tool-call parsing, and payload segmentation.
/// Broadcasts chunks to the projection layer via `broadcast::Sender<LlmChunk>`.
/// A `history_transform` may be registered to compress or augment the
/// conversation without changing the mirror history.
///
/// Refs: docs/SPECS.md §Book III-A, I-Core-ChunkBudget
pub struct OpenAiLlmClient {
    config: OpenAiConfig,
    http: reqwest::Client,
    tools_schema: Arc<RwLock<Vec<serde_json::Value>>>,
    ui_tx: broadcast::Sender<LlmChunk>,
    history: SharedHistory,
    history_transform: Arc<std::sync::RwLock<Option<HistoryTransform>>>,
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
            history_transform: Arc::clone(&self.history_transform),
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
    /// Refs: docs/SPECS.md §Book III-B
    pub fn new(config: OpenAiConfig) -> (Self, broadcast::Receiver<LlmChunk>, SharedHistory) {
        let http = match reqwest::Client::builder()
            // No global request timeout — streaming generations can
            // take minutes (e.g. 80KB file writes). Idle detection
            // is handled by the per-chunk READ_TIMEOUT in call_llm().
            .build()
        {
            Ok(c) => c,
            Err(_) => reqwest::Client::new(),
        };

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
            history_transform: Arc::new(std::sync::RwLock::new(None)),
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
    /// Refs: docs/SPECS.md §Book III-B
    pub fn subscribe(&self) -> broadcast::Receiver<LlmChunk> {
        self.ui_tx.subscribe()
    }

    /// Push a message into the history mirror.
    ///
    /// The CLI calls this method before sending a `UserMessage`
    /// to the shell, ensuring the LLM client sees the complete history.
    ///
    /// # Cancel safety
    /// This future holds an `RwLock` write guard across a single
    /// non-awaiting statement. Dropping it before await completion
    /// leaves `history` unchanged.
    pub async fn push_message(&self, message: ChatMessage) {
        self.history.write().await.push(message);
    }

    /// Update the available tools list without rebuilding the client.
    ///
    /// This list is read at the start of each `call_llm()` invocation.
    ///
    /// # Cancel safety
    /// This future holds an `RwLock` write guard across a single
    /// non-awaiting statement. Dropping it before await completion
    /// leaves `tools_schema` unchanged.
    pub async fn set_tools_schema(&self, schemas: Vec<serde_json::Value>) {
        let mut guard = self.tools_schema.write().await;
        *guard = schemas;
    }

    /// Set a transform applied to the history before each LLM request.
    ///
    /// The mirror history is left untouched so the UI and persistence still
    /// see the full conversation. Only the request payload sent to the provider
    /// is transformed.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn set_history_transform(&self, transform: Option<HistoryTransform>) {
        let mut guard = match self.history_transform.write() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        *guard = transform;
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
    /// Refs: docs/SPECS.md §Book III-B
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
    ///
    /// # Cancel safety
    /// This future holds an `RwLock` write guard across a single
    /// non-awaiting loop. Dropping it before await completion may
    /// leave `history` partially updated; callers should retry the
    /// full slice on recovery.
    pub async fn push_tool_results(&self, results: &[ToolResultDTO]) {
        let mut history = self.history.write().await;
        for result in results {
            let content = match &result.outcome {
                ToolOutcome::Success(s)
                | ToolOutcome::BusinessError(s)
                | ToolOutcome::SystemError(s) => s.clone(),
                ToolOutcome::TimeoutWithPartialData {
                    partial_output: Some(s),
                } => s.clone(),
                ToolOutcome::TimeoutWithPartialData {
                    partial_output: None,
                } => String::new(),
                _ => String::new(),
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
struct StreamOutcome {
    finish_reason: Option<String>,
    tool_calls: BTreeMap<usize, ToolCallAccumulator>,
    content_chars: usize,
    reasoning_chars: usize,
}

impl OpenAiLlmClient {
    /// Build the HTTP request from current history and tools.
    ///
    /// # Complexity
    /// O(n) where n = history length (build_messages copies).
    async fn build_request(&self) -> (serde_json::Value, usize) {
        let history_guard = self.history.read().await;
        let transformed = {
            let transform_guard = match self.history_transform.read() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            match transform_guard.as_ref() {
                Some(transform) => transform(&history_guard),
                None => history_guard.iter().cloned().collect(),
            }
        };
        let messages = crate::request::build_messages(&transformed);
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
            true,
        );
        (body, msg_count)
    }

    /// Build a non-streaming summarization request for the given messages.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn build_summary_request(&self, messages: &[ChatMessage]) -> serde_json::Value {
        let summary_prompt = ChatMessage::System {
            content: "Summarize the following conversation concisely. \
                Preserve key facts, user intent, and any decisions or tool results. \
                The summary will replace the messages it covers."
                .into(),
        };
        let mut request_messages = crate::request::build_messages(&[summary_prompt]);
        request_messages.extend(crate::request::build_messages(messages));
        crate::request::build_request_body(
            &self.config.model,
            request_messages,
            self.config.max_tokens.min(512),
            None,
            None,
            false,
        )
    }

    /// Extract the assistant content from a non-streaming completion response.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn extract_summary_text(body: &serde_json::Value) -> Option<String> {
        body.get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?
            .as_str()
            .map(|s| s.to_string())
    }

    /// Send the request and surface network or HTTP errors.
    ///
    /// On error emits `SystemSignal::NetworkUnavailable` and
    /// returns `Ok(())` so the kernel can recover.
    async fn send_request(
        &self,
        shell: &BriocheShell,
        body: &serde_json::Value,
        url: &str,
    ) -> Result<reqwest::Response, ShellError> {
        let body_str = body.to_string();

        let _ = self.ui_tx.send(LlmChunk::Status(format!(
            "HTTP POST {} — ~{} chars",
            url,
            body_str.len(),
        )));

        let request = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(body);

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
                return Err(OpenAiError::Network(err.to_string()).into());
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = limited_error_body(response, MAX_ERROR_BODY_BYTES).await;
            let compact = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                json.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map_or_else(|| body_text.clone(), |s| s.to_string())
            } else {
                match body_text.lines().next() {
                    Some(line) => line.to_string(),
                    None => body_text.clone(),
                }
            };
            let msg = format!("HTTP {status}: {compact}");
            tracing::error!(status = %status, error = %body_text, "OpenAI HTTP error");
            let _ = self.ui_tx.send(LlmChunk::Error(msg.clone()));
            shell
                .send_system_signal(SystemSignal::NetworkUnavailable { reason: msg })
                .await?;
            return Err(OpenAiError::Http {
                status: status.as_u16(),
                message: compact,
            }
            .into());
        }

        Ok(response)
    }

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
    async fn read_sse_stream(
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
    async fn finalize_assistant_message(&self, outcome: StreamOutcome) -> Result<(), ShellError> {
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

// ---------------------------------------------------------------------------
// Diagnostic helpers — private, redacted request dumps.
// ---------------------------------------------------------------------------

/// Maximum size of a redacted diagnostic request body, in bytes.
const MAX_DIAG_BYTES: usize = 1_048_576;

/// Diagnostic marker for redacted text fields.
const REDACTED: &str = "[REDACTED]";

/// Returns the private diagnostic directory, creating it with 0700 if needed.
///
/// Uses `$XDG_CACHE_HOME/brioche/diag` when available, otherwise
/// falls back to `$HOME/.cache/brioche/diag`.
fn private_diag_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                let mut path = PathBuf::from(home);
                path.push(".cache");
                path
            })
        })?;

    let mut dir = base;
    dir.push("brioche");
    dir.push("diag");

    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "failed to create diagnostic directory");
        return None;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&dir).ok()?;
        let mut perms = metadata.permissions();
        // Ensure the directory is not world-readable/searchable.
        let mode = perms.mode() & 0o777;
        if mode & 0o077 != 0 {
            perms.set_mode(0o700);
            if let Err(e) = std::fs::set_permissions(&dir, perms) {
                tracing::warn!(error = %e, "failed to set diagnostic directory permissions");
                return None;
            }
        }
    }

    Some(dir)
}

/// Recursively redact sensitive string fields from a request body.
///
/// Redacts `content` in messages and `description` in tool function
/// definitions. Leaves structural metadata intact for debugging.
fn redact_request_body(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let redacted = if k == "content" || k == "description" {
                    match v {
                        serde_json::Value::String(_) => serde_json::Value::String(REDACTED.into()),
                        _ => redact_request_body(v),
                    }
                } else {
                    redact_request_body(v)
                };
                out.insert(k.clone(), redacted);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(redact_request_body).collect())
        }
        other => other.clone(),
    }
}

/// Writes a redacted, size-capped request body to the private diagnostic dir.
fn write_diag_request(turn: usize, body: &serde_json::Value) {
    let Some(dir) = private_diag_dir() else {
        return;
    };

    let mut path = dir;
    path.push(format!("brioche_request_turn_{turn}.json"));

    let redacted = redact_request_body(body);
    let mut text = redacted.to_string();
    const TRUNCATION_SUFFIX: &str = "\n...[truncated]";
    if text.len() > MAX_DIAG_BYTES {
        let limit = MAX_DIAG_BYTES.saturating_sub(TRUNCATION_SUFFIX.len());
        let trunc_idx = text.floor_char_boundary(limit);
        text.truncate(trunc_idx);
        text.push_str(TRUNCATION_SUFFIX);
    }

    if let Err(e) = std::fs::write(&path, &text) {
        tracing::warn!(error = %e, path = %path.display(), "failed to write diagnostic request");
    }
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
        let _ = self
            .ui_tx
            .send(LlmChunk::Status(format!("Calling LLM (turn {turn})…")));

        let (body, _msg_count) = self.build_request().await;

        // Diagnostic: write redacted request body to a private cache directory.
        // Activated by the BRIOCHE_DIAG env var (any value).
        if std::env::var("BRIOCHE_DIAG").is_ok() {
            write_diag_request(turn, &body);
        }

        let response = match self.send_request(shell, &body, &url).await {
            Ok(r) => r,
            Err(_) => return Ok(()), // Error already surfaced to shell.
        };

        let stream = response.bytes_stream();
        let outcome = match self.read_sse_stream(shell, stream, turn).await {
            Ok(o) => o,
            Err(_) => return Ok(()), // Error already surfaced to shell.
        };

        if outcome.finish_reason.as_deref() == Some("tool_calls")
            && let Some(first) = outcome.tool_calls.values().next()
        {
            self.emit_tool_call_done(shell, &first.id).await?;
        }

        self.finalize_assistant_message(outcome).await?;

        shell
            .send_input(EngineInput::LlmStream(StreamEvent::Done))
            .await?;
        let _ = self.ui_tx.send(LlmChunk::Done);
        Ok(())
    }

    async fn summarize(
        &self,
        shell: &BriocheShell,
        messages: &[ChatMessage],
    ) -> Result<ChatMessage, ShellError> {
        if messages.is_empty() {
            return Ok(ChatMessage::System {
                content: "[no messages to summarize]".into(),
            });
        }

        let _ = self
            .ui_tx
            .send(LlmChunk::Status("Summarizing conversation…".into()));

        let url = format!("{}/chat/completions", self.config.base_url);
        let body = self.build_summary_request(messages);

        let response = self.send_request(shell, &body, &url).await?;
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|err| OpenAiError::SummaryParse(err.to_string()))?;

        match Self::extract_summary_text(&json) {
            Some(content) => Ok(ChatMessage::System { content }),
            None => Ok(ChatMessage::System {
                content: "[summary unavailable]".into(),
            }),
        }
    }

    async fn push_tool_results(&self, results: &[ToolResultDTO]) {
        OpenAiLlmClient::push_tool_results(self, results).await;
    }
}
#[cfg(test)]
mod tests {
    use brioche_shell_runtime::ShellError;

    use super::OpenAiError;

    #[test]
    fn openai_error_network_preserves_context() {
        let err = OpenAiError::Network("connection refused".into());
        let shell_err: ShellError = err.into();
        let msg = format!("{shell_err}");
        assert!(msg.contains("connection refused"), "{msg}");
    }

    #[test]
    fn openai_error_http_preserves_status_and_message() {
        let err = OpenAiError::Http {
            status: 503,
            message: "overloaded".into(),
        };
        let shell_err: ShellError = err.into();
        let msg = format!("{shell_err}");
        assert!(msg.contains("503") && msg.contains("overloaded"), "{msg}");
    }

    #[test]
    fn openai_error_sse_preserves_message() {
        let err = OpenAiError::Sse("stream closed".into());
        let shell_err: ShellError = err.into();
        let msg = format!("{shell_err}");
        assert!(msg.contains("stream closed"), "{msg}");
    }
}

#[cfg(test)]
mod diag_tests {
    use super::*;

    fn obj(entries: &[(&str, serde_json::Value)]) -> serde_json::Value {
        serde_json::Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn arr(values: &[serde_json::Value]) -> serde_json::Value {
        serde_json::Value::Array(values.to_vec())
    }

    fn s(value: &str) -> serde_json::Value {
        serde_json::Value::String(value.into())
    }

    #[test]
    fn redact_request_body_obscures_message_content() {
        let body = obj(&[
            ("model", s("gpt-4o")),
            (
                "messages",
                arr(&[
                    obj(&[
                        ("role", s("system")),
                        ("content", s("secret system prompt")),
                    ]),
                    obj(&[("role", s("user")), ("content", s("secret user message"))]),
                ]),
            ),
            (
                "tools",
                arr(&[obj(&[
                    ("type", s("function")),
                    (
                        "function",
                        obj(&[
                            ("name", s("read_file")),
                            ("description", s("secret tool description")),
                        ]),
                    ),
                ])]),
            ),
        ]);

        let redacted = redact_request_body(&body);
        assert_eq!(redacted["model"], s("gpt-4o"));
        assert_eq!(redacted["messages"][0]["content"], s(REDACTED));
        assert_eq!(redacted["messages"][1]["content"], s(REDACTED));
        assert_eq!(redacted["tools"][0]["function"]["description"], s(REDACTED));
        assert_eq!(redacted["tools"][0]["function"]["name"], s("read_file"));
    }

    #[test]
    fn redact_request_body_leaves_non_sensitive_values_intact() {
        let body = obj(&[
            ("model", s("gpt-4o")),
            ("stream", serde_json::Value::Bool(true)),
            ("max_tokens", serde_json::Value::Number(4096.into())),
        ]);

        let redacted = redact_request_body(&body);
        assert_eq!(redacted["model"], s("gpt-4o"));
        assert_eq!(redacted["stream"], serde_json::Value::Bool(true));
        assert_eq!(
            redacted["max_tokens"],
            serde_json::Value::Number(4096.into())
        );
    }
}
