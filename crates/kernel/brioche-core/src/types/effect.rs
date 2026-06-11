//! Engine inputs, effects, and policy decisions.
//!
//! The core effect enum `Effect`, UI widgets, engine inputs, and the
//! policy decision types that govern transition behaviour.

use serde::{Deserialize, Serialize};

use super::fundamental::{PluginError, PluginSource, SubRoutineHandle, TaskId};
use super::runtime::StreamEvent;
use super::session::ChatMessage;
use super::tool::{ActiveToolCall, ToolResultDTO};

// ---------------------------------------------------------------------------
// EngineInput
// ---------------------------------------------------------------------------

/// High-level input to the synchronous kernel.
///
/// System signals, async results, and governance notifications transit
/// through **separate channels** (see SPECS.md §1.4) and are **not**
/// variants of `EngineInput`.
///
/// Refs: I-Core-EngineInput
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EngineInput {
    /// User message. Triggers `Idle -> Predicting` transition.
    UserMessage(String),
    /// LLM stream fragments.
    LlmStream(StreamEvent),
    /// Tool execution results (parallelized by the shell).
    ToolCallsResult {
        /// Must match the current epoch or the result is rejected.
        generation_id: u64,
        /// Parallel tool execution outcomes.
        results: Vec<ToolResultDTO>,
    },
    /// Request to hydrate a sub-routine into the `SessionRegistry`.
    RestoreSubRoutine {
        /// Handle for the restored sub-routine session.
        handle: SubRoutineHandle,
        /// Serialized session head (postcard-encoded `SessionHeadDTO`).
        head_blob: Vec<u8>,
    },
}

// ---------------------------------------------------------------------------
// PolicyDecision
// ---------------------------------------------------------------------------

/// Decision returned by a plugin hook, interpreted by the kernel.
///
/// Refs: I-Gov-Decision-Required, I-Gov-OverrideTrace
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PolicyDecision {
    /// Allow the current operation to proceed.
    Allow,
    /// Block the current operation with a reason.
    Block {
        /// Human-readable explanation for the block.
        reason: String,
    },
    /// Mutate the session history before the next phase.
    MutateHistory(Vec<HistoryEdit>),
    /// Request emission of a mechanical effect.
    /// Validated by `HookEffectConstraint` if injected.
    RequestEffect(Effect),
    /// Force a state transition and emit associated effects.
    OverrideTransition(Vec<Effect>),
}

/// Individual history edit operation.
///
/// Applied sequentially in plugin evaluation order. The kernel validates
/// indices after each edit to prevent out-of-bounds mutations.
///
/// Refs: I-Gov-Decision-Isolation
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HistoryEdit {
    /// Insert a message at a specific history index.
    Insert {
        /// Position in history for the edit operation.
        index: usize,
        /// The `ChatMessage` to insert or replace.
        message: ChatMessage,
    },
    /// Overwrite a message at a specific history index.
    Replace {
        /// Position in history for the edit operation.
        index: usize,
        /// The `ChatMessage` to insert or replace.
        message: ChatMessage,
    },
    /// Discard all but the most recent N messages.
    Truncate {
        /// Number of most recent messages to retain.
        keep_last: usize,
    },
}

// ---------------------------------------------------------------------------
// UiWidget
// ---------------------------------------------------------------------------

/// Structured UI widget emitted via `Effect::ForwardToUi`.
///
/// Replaces the previous `String` + `serde_json::Value` anti-pattern with
/// exhaustively matchable domain types. Third-party widgets that do not
/// match a known shape fall back to `UiWidget::Custom`.
///
/// The projection layer can still match on canonical widget type strings
/// via `UiWidget::widget_type()` during migration; new code should match
/// on enum variants directly.
///
/// Refs: I-Comp-Typed-Effects
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum UiWidget {
    /// Text fragment from LLM streaming.
    TextChunk {
        /// Correlation ID for the current LLM stream.
        trace_id: String,
        /// Fragment of generated text.
        text: String,
    },
    /// Generic error notification displayed in the content area.
    Error {
        /// Error code for classification and retry logic.
        code: String,
        /// Human-readable error description.
        message: String,
    },
    /// Critical system error (e.g., governance cascade failure).
    CriticalError {
        /// Name of the subsystem that failed.
        component: String,
        /// Optional technical detail for debugging.
        detail: Option<String>,
    },
    /// System degradation banner (e.g., plugin quarantined).
    SystemDegraded {
        /// Name of the quarantined or failing plugin.
        plugin: String,
    },
    /// Network unavailability notification.
    NetworkError {
        /// Transport-level failure description.
        reason: String,
    },
    /// Generic status indicator (e.g., "cancelled").
    Status(String),
    /// Sub-routine timeout notification.
    SubRoutineTimeout {
        /// Handle for the restored sub-routine session.
        handle: SubRoutineHandle,
        /// Timeout limit that was exceeded.
        limit_ms: u64,
    },
    /// Sub-routine successfully restored.
    SubRoutineLoaded {
        /// Handle of the restored sub-routine.
        handle: SubRoutineHandle,
    },
    /// Pending task status update.
    PendingTask {
        /// Identifier of the background task.
        task_id: String,
        /// Current status string (e.g., "running", "completed").
        status: String,
    },
    /// Test widget for integration tests.
    Test {
        /// Test message payload.
        msg: String,
    },
    /// Catch-all for unknown third-party widgets.
    ///
    /// Payload is raw JSON bytes to preserve determinism.
    /// The projection layer deserializes on the shell side.
    ///
    /// Refs: I-Comp-Typed-Effects
    Custom {
        /// Canonical type string for third-party widget routing.
        widget_type: String,
        /// Raw JSON payload. Deterministic because it is bytes.
        payload_json: Vec<u8>,
    },
}

impl UiWidget {
    /// Returns the canonical widget type string.
    ///
    /// Used by the projection layer for registry lookup and priority
    /// classification while the ecosystem migrates to structured variants.
    ///
    /// Complexity: O(1).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Comp-Typed-Effects
    pub fn widget_type(&self) -> &str {
        match self {
            UiWidget::TextChunk { .. } => "text_chunk",
            UiWidget::Error { .. } => "error",
            UiWidget::CriticalError { .. } => "critical_error",
            UiWidget::SystemDegraded { .. } => "system_degraded",
            UiWidget::NetworkError { .. } => "network_error",
            UiWidget::Status(_) => "status",
            UiWidget::SubRoutineTimeout { .. } => "subroutine_timeout",
            UiWidget::SubRoutineLoaded { .. } => "subroutine_loaded",
            UiWidget::PendingTask { .. } => "pending_task",
            UiWidget::Test { .. } => "test",
            UiWidget::Custom { widget_type, .. } => widget_type,
        }
    }
}

// ---------------------------------------------------------------------------
// Effect
// ---------------------------------------------------------------------------

/// Structured error payload for `Effect::Error`.
///
/// Replaces the previous `message: String` anti-pattern with typed,
/// exhaustively matchable variants. The shell and projection layer can
/// inspect specific error scenarios without string parsing.
///
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
/// Refs: I-Comp-Typed-Effects
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorDetail {
    /// Fallback for errors that do not yet have a structured variant.
    Generic(String),
    /// History edit index out of bounds.
    HistoryIndexOutOfBounds {
        /// Which edit failed: insert, replace, or truncate.
        operation: String,
        /// Position in history for the edit operation.
        index: usize,
        /// Current history length at the time of the failed edit.
        len: usize,
    },
    /// Tool descriptor missing timeout (default applied).
    MissingToolTimeout {
        /// Default timeout applied when the descriptor omits one.
        default_timeout_ms: u64,
    },
    /// Effect variant not allowed on the current hook.
    EffectNotAllowed {
        /// Name of the hook on which the effect was requested.
        hook: String,
        /// Name of the disallowed `Effect` variant.
        effect_variant: String,
    },
    /// Effects were dropped after `RebuildRoutes`.
    EffectsDroppedAfterRebuildRoutes {
        /// Number of discarded effects.
        count: usize,
    },
    /// Sub-routine lifecycle guard failed.
    SubRoutineLifecycleFailed {
        /// Name of the lifecycle guard that failed.
        guard_name: String,
    },
    /// State inconsistency detected by a governance plugin or internal check.
    StateInconsistent {
        /// Source of the inconsistency (plugin name or internal module).
        source: String,
    },
}

impl std::fmt::Display for ErrorDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorDetail::Generic(s) => write!(f, "{}", s),
            ErrorDetail::HistoryIndexOutOfBounds {
                operation,
                index,
                len,
            } => {
                write!(
                    f,
                    "history {} index {} out of bounds (len={})",
                    operation, index, len
                )
            }
            ErrorDetail::MissingToolTimeout { default_timeout_ms } => {
                write!(
                    f,
                    "Missing timeout, applied default {} ms",
                    default_timeout_ms
                )
            }
            ErrorDetail::EffectNotAllowed {
                hook,
                effect_variant,
            } => {
                write!(f, "Effect {} not allowed on hook {}", effect_variant, hook)
            }
            ErrorDetail::EffectsDroppedAfterRebuildRoutes { count } => {
                write!(f, "{} effect(s) dropped after RebuildRoutes", count)
            }
            ErrorDetail::SubRoutineLifecycleFailed { guard_name } => {
                write!(f, "Sub-routine lifecycle guard '{}' failed", guard_name)
            }
            ErrorDetail::StateInconsistent { source } => {
                write!(f, "State inconsistent: {}", source)
            }
        }
    }
}

/// Declarative effect emitted by the kernel. The shell is responsible for
/// execution.
///
/// `Effect` contains **only** pure mechanical effects. No telemetry,
/// UI fallback, or specific notification variants appear here.
///
/// Refs: I-Core-EffectPure, I-Core-RetVecEffect
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Effect {
    /// Request the shell to initiate an LLM prediction.
    CallLlmNetwork,
    /// Request the shell to execute active tool calls.
    ExecuteTools(Vec<ActiveToolCall>),
    /// Emit a structured widget to the projection layer.
    ForwardToUi(UiWidget),
    /// Report a system-level error. The shell decides on recovery.
    Error {
        /// Error code for classification and retry logic.
        code: ErrorCode,
        /// Optional technical detail for debugging.
        detail: ErrorDetail,
    },
    /// Persist the current session head to disk (Delta protocol).
    SaveSession,
    /// Persist a plugin-specific binary blob.
    SavePluginBlob {
        /// Plugin that owns this blob.
        plugin_id: PluginSource,
        /// Opaque binary payload. Serialized by the plugin itself.
        data: Vec<u8>,
    },
    /// Start a background summarization task.
    TriggerSummarization,
    /// Offload a CPU-intensive computation to the shell.
    ExecuteCpuTask {
        /// Identifier of the background task.
        task_id: TaskId,
        /// Serialized input for the offloaded computation.
        payload: Vec<u8>,
    },
    /// Request garbage collection of orphaned sub-routines.
    TriggerGc,
    /// Notify the shell that the kernel is idle and awaiting input.
    SystemIdle,
    /// A plugin fatally errored. Triggers quarantine evaluation.
    PluginFault {
        /// Plugin that faulted.
        plugin_name: PluginSource,
        /// The fatal error that triggered this notification.
        error: PluginError,
    },
    /// Rebuild the plugin routing table (after quarantine).
    RebuildRoutes,
    /// A sub-routine was successfully hydrated from disk.
    SubRoutineRestored {
        /// Handle for the restored sub-routine session.
        handle: SubRoutineHandle,
    },
}

/// Mechanical error codes carried by `Effect::Error`.
///
/// These are **not** plugin errors; they represent system-level conditions
/// that the shell must handle.
///
/// Refs: I-Core-NoPanic
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Transport-level network failure.
    NetworkUnavailable,
    /// User cancelled the current operation.
    OperationCancelled,
    /// Internal state violates an invariant.
    StateInconsistency,
    /// Async response carries a stale generation ID.
    EpochMismatch,
    /// A governance plugin crashed or returned a fatal error.
    PluginFaulted,
}

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// EffectBit
// ---------------------------------------------------------------------------

/// Bitmask constants for each `Effect` variant, used by `HookEffectConstraint`
/// for O(1) permission validation.
/// # Panics
/// Never panics.
///
/// Refs: I-Core-HookEffect-O1
pub struct EffectBit;

impl EffectBit {
    /// Bit for `Effect::CallLlmNetwork`.
    pub const CALL_LLM_NETWORK: u64 = 1 << 0;
    /// Bit for `Effect::Error`.
    pub const ERROR: u64 = 1 << 3;
    /// Bit for `Effect::ExecuteCpuTask`.
    pub const EXECUTE_CPU_TASK: u64 = 1 << 7;
    /// Bit for `Effect::ExecuteTools`.
    pub const EXECUTE_TOOLS: u64 = 1 << 1;
    /// Bit for `Effect::ForwardToUi`.
    pub const FORWARD_TO_UI: u64 = 1 << 2;
    /// Bit for `Effect::PluginFault`.
    pub const PLUGIN_FAULT: u64 = 1 << 10;
    /// Bit for `Effect::RebuildRoutes`.
    pub const REBUILD_ROUTES: u64 = 1 << 11;
    /// Bit for `Effect::SavePluginBlob`.
    pub const SAVE_PLUGIN_BLOB: u64 = 1 << 5;
    /// Bit for `Effect::SaveSession`.
    pub const SAVE_SESSION: u64 = 1 << 4;
    /// Bit for `Effect::SubRoutineRestored`.
    pub const SUB_ROUTINE_RESTORED: u64 = 1 << 12;
    /// Bit for `Effect::SystemIdle`.
    pub const SYSTEM_IDLE: u64 = 1 << 9;
    /// Bit for `Effect::TriggerGc`.
    pub const TRIGGER_GC: u64 = 1 << 8;
    /// Bit for `Effect::TriggerSummarization`.
    pub const TRIGGER_SUMMARIZATION: u64 = 1 << 6;
    // Bits 13-63 reserved for future extensions.
}

/// Map an `Effect` to its bitmask constant.
///
/// Complexity: O(1). Match on enum variant.
/// # Panics
/// Never panics.
///
/// Refs: I-Core-HookEffect-O1
pub fn effect_to_bitmask(effect: &Effect) -> u64 {
    match effect {
        Effect::CallLlmNetwork => EffectBit::CALL_LLM_NETWORK,
        Effect::ExecuteTools(_) => EffectBit::EXECUTE_TOOLS,
        Effect::ForwardToUi(_) => EffectBit::FORWARD_TO_UI,
        Effect::Error { .. } => EffectBit::ERROR,
        Effect::SaveSession => EffectBit::SAVE_SESSION,
        Effect::SavePluginBlob { .. } => EffectBit::SAVE_PLUGIN_BLOB,
        Effect::TriggerSummarization => EffectBit::TRIGGER_SUMMARIZATION,
        Effect::ExecuteCpuTask { .. } => EffectBit::EXECUTE_CPU_TASK,
        Effect::TriggerGc => EffectBit::TRIGGER_GC,
        Effect::SystemIdle => EffectBit::SYSTEM_IDLE,
        Effect::PluginFault { .. } => EffectBit::PLUGIN_FAULT,
        Effect::RebuildRoutes => EffectBit::REBUILD_ROUTES,
        Effect::SubRoutineRestored { .. } => EffectBit::SUB_ROUTINE_RESTORED,
    }
}

// ---------------------------------------------------------------------------
