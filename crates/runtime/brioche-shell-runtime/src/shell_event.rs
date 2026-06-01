//! Standardized event bus for the shell projection layer.
//!
//! All significant state changes, errors, and progress updates are
//! broadcast as `ShellEvent` so that every consumer (headless CLI,
//! interactive TUI, desktop GUI, logs) can render them consistently.
//!
//! # Design principles
//! - **One channel, many consumers**: a single `broadcast::Sender<ShellEvent>`
//!   is held by the `DefaultEffectExecutor` and `OpenAiLlmClient`.
//! - **Structured over stringly**: variants carry typed data; formatting
//!   is the consumer's responsibility.
//! - **Recoverable flag**: `Error::recoverable` lets the UI decide
//!   whether to abort the session or continue.
//!
//! Refs: SPECS.md §Book III-A Ch 1.2

/// Event broadcast from the shell runtime to all projection consumers.
#[derive(Clone, Debug)]
pub enum ShellEvent {
    // ------------------------------------------------------------------
    // LLM streaming
    // ------------------------------------------------------------------
    /// Text fragment from the LLM response stream.
    LlmText(String),
    /// A tool call has started (id + name now known).
    LlmToolCallStart { id: String, name: String },
    /// Partial argument JSON for an in-flight tool call.
    LlmToolArgument { id: String, fragment: String },
    /// All tool calls for this turn are complete.
    LlmToolCallDone { id: String },
    /// The LLM stream finished (either text or tool_calls).
    LlmDone,

    // ------------------------------------------------------------------
    // Tool execution
    // ------------------------------------------------------------------
    /// A tool finished executing and produced output.
    ToolResult { name: String, output: String },

    // ------------------------------------------------------------------
    // Status / progress
    // ------------------------------------------------------------------
    /// Transient status message (e.g. "Calling LLM…", "Executing 3 tools…").
    Status { message: String },
    /// The agent is waiting for the LLM to respond.
    ///
    /// Consumers should render this as a spinner or single-line
    /// indicator that is replaced when the first `LlmText` or
    /// `LlmToolCallStart` arrives.
    Thinking { message: String },

    // ------------------------------------------------------------------
    // Errors and warnings
    // ------------------------------------------------------------------
    /// A recoverable or fatal error occurred.
    ///
    /// # Fields
    /// - `code`: machine-readable code (`EpochMismatch`, `ToolTimeout`, …).
    /// - `message`: human-readable explanation.
    /// - `source`: which subsystem emitted it (`epoch_guard`, `tool_executor`, …).
    /// - `recoverable`: `true` if the session can continue; `false` to abort.
    /// - `suggestion`: actionable hint for the user (e.g. "→ Set BRIOCHE_API_KEY").
    Error {
        code: String,
        message: String,
        source: &'static str,
        recoverable: bool,
        suggestion: Option<String>,
    },
    /// A non-fatal warning.
    Warning {
        message: String,
        source: &'static str,
    },
}

/// Default formatting for headless / log consumers.
///
/// Interactive consumers (TUI, GUI) should render with colours/icons.
impl std::fmt::Display for ShellEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellEvent::LlmText(text) => write!(f, "{text}"),
            ShellEvent::LlmToolCallStart { id, name } => {
                write!(f, "[tool] {name} (id={id}) starting")
            }
            ShellEvent::LlmToolArgument { id, fragment } => {
                write!(f, "[tool] {id} arg += {fragment}")
            }
            ShellEvent::LlmToolCallDone { id } => write!(f, "[tool] {id} done"),
            ShellEvent::LlmDone => write!(f, "[llm] stream done"),
            ShellEvent::ToolResult { name, output } => {
                write!(f, "[tool] {name} result:\n{output}")
            }
            ShellEvent::Status { message } => write!(f, "[status] {message}"),
            ShellEvent::Thinking { message } => write!(f, "[thinking] {message}"),
            ShellEvent::Error {
                code,
                message,
                source,
                recoverable,
                suggestion,
            } => {
                let severity = if *recoverable { "ERROR" } else { "FATAL" };
                write!(f, "[{severity}][{source}] {code}: {message}")?;
                if let Some(hint) = suggestion {
                    write!(f, "\n  → {hint}")?;
                }
                Ok(())
            }
            ShellEvent::Warning { message, source } => {
                write!(f, "[WARN][{source}] {message}")
            }
        }
    }
}

// ------------------------------------------------------------------
// Helper constructors for ergonomic event creation
// ------------------------------------------------------------------

impl ShellEvent {
    /// Create an error event with an optional suggestion.
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        source: &'static str,
        recoverable: bool,
        suggestion: Option<String>,
    ) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
            source,
            recoverable,
            suggestion,
        }
    }

    /// Create a network/auth error with a standard suggestion.
    pub fn network_error(status: u16, message: impl Into<String>, source: &'static str) -> Self {
        let suggestion = match status {
            401 => Some("Check your BRIOCHE_API_KEY environment variable.".into()),
            429 => Some("Rate limited — wait a moment or switch models.".into()),
            500..=599 => Some("Provider error — retry or check service status.".into()),
            _ => None,
        };
        Self::error(format!("HTTP{status}"), message, source, true, suggestion)
    }

    /// Create a fatal error that should abort the session.
    pub fn fatal(
        code: impl Into<String>,
        message: impl Into<String>,
        source: &'static str,
        suggestion: Option<String>,
    ) -> Self {
        Self::error(code, message, source, false, suggestion)
    }
}
