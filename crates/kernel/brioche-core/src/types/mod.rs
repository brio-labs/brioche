//! Book I — The Core Book: Fundamental types for the Brioche kernel.
//!
//! This module contains `Session`, `AgentState`, `EngineInput`, `Effect`, and
//! related mechanical types. Definitions are populated incrementally across
//! Sprints 2–5.
//!
//! ## Sub-modules
//! - `fundamental`: Newtype identifiers and error types (`TaskId`, `PluginSource`,
//!   `SubRoutineHandle`, `PluginError`, `BriocheError`).
//! - `session`: Session state machine, registry, and snapshots.
//! - `tool`: Tool call descriptors, outcomes, and result DTOs.
//! - `effect`: Engine inputs, effects, UI widgets, and policy decisions.
//! - `trace`: Transition traces and rollback event logs.
//! - `runtime`: Stream events, signals, and async task results.
//!
//! Invariants upheld:
//! - I-Core-Pure: All types are deterministic and serializable.
//! - I-Core-NoPanic: Invalid state transitions produce `BriocheError`, not panics.
//! - I-Core-ActiveToolCall: `ActiveToolCall` is kernel-internal; plugins use `ToolCallDescriptor`.
//! - I-Core-RetVecEffect: `Effect` is the sole output channel of `transition()`.
//!
//! Refs: docs/SPECS.md §2, §5

pub mod effect;
pub mod fundamental;
pub mod runtime;
pub mod session;
pub mod tool;
pub mod trace;

// Re-exports preserved for backward compatibility.
// All downstream consumers import from `brioche_core::types::*` or
// `brioche_core::{AgentState, Effect, ...}`.
pub use effect::{
    Effect, EffectBit, EngineInput, ErrorCode, ErrorDetail, HistoryEdit, HistoryOperation,
    InconsistencySource, PolicyDecision, UiWidget, effect_to_bitmask,
};
pub use fundamental::{
    BriocheError, EpochAction, PluginError, PluginResult, PluginSource, SubRoutineHandle, TaskId,
};
pub use runtime::{
    AsyncTaskResult, EpochState, ExecutionPath, GovernanceNotification, SignalBuffer,
    SignalDrainBatch, StreamAction, StreamEvent, StreamToolAccumulator, SystemSignal, ToolStatus,
};
pub use session::{
    AgentState, AgentStateTag, ChatMessage, Session, SessionRegistry, SessionSnapshot,
};
pub use tool::{
    ActiveToolCall, ToolCallDescriptor, ToolOutcome, ToolResultDTO, TruncatedToolResult, seal,
    seal_single, tool_outcome_to_string,
};
pub use trace::{
    RollbackEvent, RollbackEventLog, SupersededTransitionTrace, SupersededTransitionTraceLog,
    TransitionTrace, TransitionTraceLog,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default tool timeout applied when a descriptor omits `timeout_ms`.
///
/// The kernel materializes this value during `seal()` when no plugin
/// has set a timeout on the `ToolCallDescriptor`.
///
/// Refs: I-Core-ActiveToolCall
pub const DEFAULT_TOOL_TIMEOUT_MS: u64 = 30_000;

/// Initial generation ID for predictions.
///
/// Refs: I-Core-AgentState
pub const INITIAL_GENERATION_ID: u64 = 1;

/// Maximum number of entries retained in transition trace ring buffers.
///
/// Refs: I-Gov-OverrideTrace
pub const TRACE_LOG_CAPACITY: usize = 128;

/// Maximum size of an inline streaming chunk in bytes.
///
/// SSE payloads exceeding this size are segmented into independent
/// 4 KB fragments before injection into the kernel, guaranteeing the
/// absence of heap allocation in the synchronous hot path for plugins
/// in `Pass` or `Hold` mode.
///
/// Refs: I-Core-ChunkBudget
pub const MAX_INLINE_CHUNK: usize = 4096;
