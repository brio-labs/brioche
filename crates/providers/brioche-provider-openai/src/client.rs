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

mod diagnostics;
mod errors;
mod request_flow;
mod retry;
mod stream;

use std::sync::Arc;

use brioche_core::{ChatMessage, MAX_INLINE_CHUNK, StreamEvent, ToolOutcome, ToolResultDTO};
use brioche_shell_runtime::{BriocheShell, EngineInput, LlmChunk, LlmClient, ShellError};
use bytes::Bytes;
use reqwest::redirect::Policy;
use tokio::sync::{RwLock, broadcast};

use crate::config::OpenAiConfig;
use crate::extractor::{ChunkExtractor, StreamErrorDetector};

use diagnostics::write_diag_request;
pub use errors::OpenAiError;
pub use retry::{MAX_ERROR_BODY_BYTES, RetryConfig};
pub(super) use retry::{is_retriable_status, limited_error_body, parse_retry_after};

/// OpenAI-compatible LLM client.
///
/// `tools_schema` is updated dynamically by the assembler (CLI)
/// when the tool registry changes. `Arc<RwLock>` allows updates
/// without reconstructing the client.
///
/// # Usage
/// ```ignore
/// let (client, llm_rx, _history) = OpenAiLlmClient::new(config)?;
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
    retry_config: RetryConfig,
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
            retry_config: self.retry_config,
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
    /// # Errors
    /// Returns `OpenAiError::HttpClientBuilder` if the underlying
    /// `reqwest` client cannot be constructed.
    ///
    /// # Panics
    /// Never panics. Empty `api_key` is accepted (some local endpoints
    /// like Ollama do not require a key).
    /// Refs: docs/SPECS.md §Book III-B
    pub fn new(
        config: OpenAiConfig,
    ) -> Result<(Self, broadcast::Receiver<LlmChunk>, SharedHistory), OpenAiError> {
        let http = reqwest::Client::builder()
            // Streaming generations may legitimately take minutes, so the
            // request timeout is enforced as time-to-first-byte in send_request.
            .redirect(Policy::limited(3))
            .build()
            .map_err(OpenAiError::HttpClientBuilder)?;

        let (ui_tx, ui_rx) = broadcast::channel(256);
        let history: SharedHistory = Arc::new(RwLock::new(Vec::new()));
        let chunk_extractor = Arc::from(crate::extractor::chunk_extractor_for_model(&config.model));
        let error_detector = Arc::from(crate::extractor::error_detector_for_model(&config.model));

        let client = Self {
            config,
            http,
            retry_config: RetryConfig::default(),
            tools_schema: Arc::new(RwLock::new(Vec::new())),
            ui_tx,
            history: Arc::clone(&history),
            history_transform: Arc::new(std::sync::RwLock::new(None)),
            pending_text: tokio::sync::Mutex::new(String::new()),
            pending_reasoning_text: tokio::sync::Mutex::new(String::new()),
            chunk_extractor,
            error_detector,
        };

        Ok((client, ui_rx, history))
    }

    /// Subscribe to the LLM chunk broadcast channel.
    ///
    /// Each call returns an independent new receiver.
    /// Refs: docs/SPECS.md §Book III-B
    pub fn subscribe(&self) -> broadcast::Receiver<LlmChunk> {
        self.ui_tx.subscribe()
    }

    /// Replace the retry/backoff policy used for provider requests.
    ///
    /// The default policy retries twice with exponential backoff starting
    /// at one second. Tests use this setter to exercise retries quickly.
    ///
    /// Refs: docs/SPECS.md §Book III-B
    pub fn with_retry_policy(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
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

    /// Returns the currently registered tool schemas.
    ///
    /// Exposed primarily for tests that need to verify the schema list
    /// after initialization.
    ///
    /// Refs: docs/SPECS.md §Book III-B, I-Shell-Runtime-OnlyIO
    ///
    /// # Cancel safety
    /// This future awaits an `RwLock` read lock. If it is dropped before the
    /// lock is acquired, the caller receives nothing and no guard is held. Once
    /// the lock is acquired, the guarded list is cloned in a single non-awaiting
    /// statement and the guard is released before the future resolves, so the
    /// returned value is always complete.
    pub async fn tools_schema(&self) -> Vec<serde_json::Value> {
        self.tools_schema.read().await.clone()
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

#[async_trait::async_trait]
impl LlmClient for OpenAiLlmClient {
    /// Initiate an LLM call and stream fragments back via `shell.send_input`.
    ///
    /// Builds the request, opens an SSE connection, and delegates the
    /// streaming loop to `read_sse_stream`.
    ///
    /// Refs: I-Core-ChunkBudget
    ///
    /// # Cancel safety
    /// This future delegates to `read_sse_stream` and may await network I/O.
    /// Dropping it before completion leaks the SSE connection until the
    /// read-side idle timeout or the provider closes it.
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), ShellError> {
        if let Err(err) = self.config.validate() {
            let _ = self.ui_tx.send(LlmChunk::Error(err.to_string()));
            return Err(ShellError::EffectExecution(err.to_string()));
        }

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
            Err(_) => {
                shell
                    .send_input(EngineInput::LlmStream(StreamEvent::Done))
                    .await?;
                let _ = self.ui_tx.send(LlmChunk::Done);
                return Ok(());
            }
        };

        let stream = response.bytes_stream();
        let outcome = match self.read_sse_stream(shell, stream, turn).await {
            Ok(o) => o,
            Err(_) => {
                shell
                    .send_input(EngineInput::LlmStream(StreamEvent::Done))
                    .await?;
                let _ = self.ui_tx.send(LlmChunk::Done);
                return Ok(());
            }
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

    /// Summarize a slice of chat history into a single compressed message.
    ///
    /// Mirrors the `LlmClient::summarize` contract.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Cancel safety
    /// This future may await network I/O. Dropping it before completion
    /// discards the in-flight request; no mirror history is modified.
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

    /// Push tool execution results into the conversational history.
    ///
    /// Delegates to the inherent `OpenAiLlmClient::push_tool_results`.
    ///
    /// Refs: I-Shell-ToolResult-PassThrough
    ///
    /// # Cancel safety
    /// Mirrors the `LlmClient::push_tool_results` contract: this future may
    /// leave the mirror history partially updated if dropped before completion
    /// because each result is appended independently.
    async fn push_tool_results(&self, results: &[ToolResultDTO]) {
        OpenAiLlmClient::push_tool_results(self, results).await;
    }
}
