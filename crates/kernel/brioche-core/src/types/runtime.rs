//! Runtime streaming and signal types.
//!
//! Stream events, system signals, async task results, and signal buffers
//! for the shell-runtime boundary.

use std::collections::BTreeMap;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use super::fundamental::PluginError;
use super::session::ChatMessage;
use super::tool::{ToolCallDescriptor, ToolOutcome};
use crate::BriocheExtensionType;

// Streaming
// ---------------------------------------------------------------------------

/// Execution path for nested / tree-structured stream events.
///
/// # Panics
/// Never panics.
/// Refs: I-Core-ChunkBudget
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPath {
    /// Ordered list of nested node identifiers for tree-structured output.
    pub nodes: Vec<String>,
}

/// Stream event delivered by the LLM provider.
///
/// `Bytes` is used for text fragments to avoid heap allocations in the
/// synchronous hot path. SSE payloads are pre-segmented to `MAX_INLINE_CHUNK`
/// (4096 bytes) by the shell.
///
/// Refs: I-Core-ChunkBudget, I-Core-StreamNoBranch
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StreamEvent {
    /// Fragment of LLM-generated text.
    TextChunk {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
        /// Text or argument fragment (pre-segmented to ≤ 4 KB).
        chunk: Bytes,
    },
    /// Beginning of a tool call declaration in the stream.
    ToolCallStart {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
        /// Stable identifier for the tool call or result.
        id: String,
        /// Name of the tool being invoked.
        name: String,
    },
    /// Fragment of tool call arguments (JSON).
    ToolArgumentChunk {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
        /// Stable identifier for the tool call or result.
        id: String,
        /// Text or argument fragment (pre-segmented to ≤ 4 KB).
        chunk: Bytes,
    },
    /// End of a tool call declaration.
    ToolCallDone {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
    },
    /// End-of-stream marker. Sent by the shell when the LLM response
    /// completes without further chunks or tool calls.
    Done,
    /// No-op event. Used for heartbeat / keepalive.
    Pass,
}

/// Action requested by a plugin in response to a stream event.
///
/// Refs: I-Core-StreamNoBranch
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StreamAction {
    /// Let the chunk pass through.
    Pass,
    /// Hold the chunk (buffering).
    Hold,
    /// Offload a CPU-intensive task to the shell.
    OffloadTask {
        /// Identifier of the background task.
        task_id: String,
        /// Serialized input for the offloaded computation.
        payload: Vec<u8>,
    },
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// EpochState
// ---------------------------------------------------------------------------

/// Epoch state managed by `EpochGuard` (governance) and read by the kernel
/// for trace logging.
///
/// ## Snapshot strategy
/// COW: full clone (~8 bytes). Single scalar — negligible weight.
///
/// Refs: I-Gov-Epoch-Reject
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct EpochState {
    /// Monotonically increasing generation counter.
    pub current_generation: u64,
}

// ---------------------------------------------------------------------------
// Stream tool accumulator
// ---------------------------------------------------------------------------

/// Transient accumulator for tool calls discovered during LLM streaming.
///
/// The kernel populates this as `ToolCallStart` / `ToolArgumentChunk`
/// events arrive. When `ToolCallDone` is received, the pending descriptors
/// are drained, passed through the `on_tool_calls` hook, sealed into
/// `ActiveToolCall`s, and stored in `session.active_tools`.
///
/// This type is transient (#[brioche(no_snapshot)]) — it does not need
/// COW rollback because it is reconstructed on every stream event.
///
/// Refs: I-Core-ActiveToolCall, I-Core-ChunkBudget
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct StreamToolAccumulator {
    /// Map tool_id -> partially-built descriptor.
    pub pending: BTreeMap<String, ToolCallDescriptor>,
}

// ---------------------------------------------------------------------------
// Separate channels — Book III-A
// ---------------------------------------------------------------------------

/// System signals emitted by the shell and consumed by governance plugins
/// via adapters. These events do **not** transit through `EngineInput`.
///
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
/// Refs: docs/SPECS.md §1.4, I-Shell-Network-Signal
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[non_exhaustive]
pub enum SystemSignal {
    /// Transport failure detected by the shell.
    NetworkUnavailable {
        /// Transport failure description.
        reason: String,
    },
    /// User requested cancellation of the current operation.
    #[default]
    OperationCancelled,
    /// Periodic heartbeat for timeout monitoring.
    Tick {
        /// Monotonically increasing milliseconds since session start.
        elapsed_ms: u64,
    },
    /// The engine thread became unresponsive and recovery was triggered.
    EngineUnresponsive {
        /// Recovery procedure that was triggered.
        procedure: String,
    },
}

/// Result of an asynchronous task executed by the shell.
///
/// Consumed by governance plugins via `AsyncTaskResultAdapter`.
///
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
/// Refs: docs/SPECS.md §1.4
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
pub enum AsyncTaskResult {
    /// Background summarization completed.
    SummarizationDone {
        /// Compressed chat message for history truncation.
        summary: ChatMessage,
        /// History index up to which summarization is valid.
        watermark: u32,
    },
    /// Offloaded computation finished.
    CpuTaskDone {
        /// Identifier matching the original `Effect::ExecuteCpuTask`.
        task_id: String,
        /// Serialized output of the CPU task.
        #[brioche(deterministic_order)]
        result: Vec<u8>,
    },
    /// Status update for a pending tool task.
    ToolStatusCheck {
        /// Identifier of the pending tool.
        task_id: String,
        /// Current execution status.
        status: ToolStatus,
    },
}

impl Default for AsyncTaskResult {
    fn default() -> Self {
        Self::CpuTaskDone {
            task_id: String::new(),
            result: Vec::new(),
        }
    }
}

/// Status of a pending tool task.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: docs/SPECS.md §1.4
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[non_exhaustive]
pub enum ToolStatus {
    /// Tool is still executing.
    #[default]
    Running,
    /// Tool finished (success or failure in `ToolOutcome`).
    Completed(ToolOutcome),
}

/// Governance notifications emitted by the shell.
///
/// Consumed by governance plugins (e.g. `QuarantineManager`) via
/// `GovernanceNotificationAdapter`.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: docs/SPECS.md §1.4
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
pub enum GovernanceNotification {
    /// A plugin emitted a fatal error. The shell notifies governance
    /// so that `QuarantineManager` can decide on follow-up.
    PluginFaulted {
        /// Plugin name.
        plugin_name: String,
        /// The fatal error that triggered this notification.
        error: PluginError,
    },
}

impl Default for GovernanceNotification {
    fn default() -> Self {
        Self::PluginFaulted {
            plugin_name: String::new(),
            error: PluginError::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Signal drainage — Book III-A
// ---------------------------------------------------------------------------

/// Batch of drained signals from the separate event channels.
///
/// Produced by `SignalDrainOrder::drain()` and consumed by the shell
/// to inject pending signals into `ExtensionStorage` before each
/// `transition()` cycle.
///
/// Canonical order is enforced by the `SignalDrainOrder` implementation:
/// `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: docs/SPECS.md §1.4, I-Shell-Drain-Atomic
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalDrainBatch {
    /// Drained system signals (produced first in canonical order).
    pub system_signals: Vec<SystemSignal>,
    /// Drained governance notifications (produced second).
    pub governance_notifications: Vec<GovernanceNotification>,
    /// Drained async task results (produced third).
    pub async_task_results: Vec<AsyncTaskResult>,
}

/// Transient buffer holding drained signals for plugin consumption.
///
/// The shell inserts this into `ExtensionStorage` before each
/// `transition()` cycle. Plugins read from it in their hooks.
/// It is cleared and repopulated each cycle.
///
/// Marked `#[brioche(no_snapshot)]` because it is fully reconstructed
/// each cycle; rollback of this buffer is meaningless.
///
/// Refs: I-Shell-Drain-Atomic
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct SignalBuffer {
    #[brioche(deterministic_order)]
    /// System-level events (network, cancel, tick) — produced first.
    pub system_signals: Vec<SystemSignal>,
    #[brioche(deterministic_order)]
    /// Plugin fault notifications — produced second.
    pub governance_notifications: Vec<GovernanceNotification>,
    #[brioche(deterministic_order)]
    /// Background task completions — produced third.
    pub async_task_results: Vec<AsyncTaskResult>,
}
