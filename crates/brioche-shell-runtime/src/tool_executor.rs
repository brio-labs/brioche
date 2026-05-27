//! Tool execution boundary.
//!
//! The [`ToolExecutor`] trait abstracts the invocation of external tools.
//! This allows tests to inject mock tool behaviour without depending on
//! real subprocesses or network calls.
//!
//! Refs: I-Shell-ToolResult-PassThrough

use brioche_core::{ActiveToolCall, ToolResultDTO};
use tokio_util::sync::CancellationToken;

/// Execute a single tool call asynchronously.
///
/// The shell is responsible for timeout enforcement (via `tokio::select!`)
/// and cancellation (via `CancellationToken`). The trait implementation
/// should perform the actual tool invocation and return the raw
/// `ToolResultDTO` without business-level transformation.
///
/// Refs: I-Shell-ToolResult-PassThrough
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute one `ActiveToolCall`.
    ///
    /// The `cancel` token is triggered by the shell on user cancellation
    /// or engine shutdown. Implementations should respect it at
    /// coarse-grained boundaries.
    async fn execute(&self, call: &ActiveToolCall, cancel: CancellationToken) -> ToolResultDTO;
}

// ---------------------------------------------------------------------------
// No-op / mock implementations
// ---------------------------------------------------------------------------

/// A tool executor that always returns success with the argument string echoed.
#[derive(Clone, Debug, Default)]
pub struct EchoToolExecutor;

#[async_trait::async_trait]
impl ToolExecutor for EchoToolExecutor {
    async fn execute(&self, call: &ActiveToolCall, _cancel: CancellationToken) -> ToolResultDTO {
        ToolResultDTO {
            tool_id: call.tool_id.clone(),
            tool_name: call.tool_name.clone(),
            outcome: brioche_core::ToolOutcome::Success(call.arguments.clone()),
        }
    }
}
