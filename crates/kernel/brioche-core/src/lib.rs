#![deny(clippy::unwrap_used, clippy::expect_used)]

//! Book I — The Core Book: Synchronous kernel and pure mechanisms.
//!
//! This crate upholds:
//! - I-Core-ExtensionType: Compile-time verified extension types via `BriocheExtensionType`.
//! - I-Core-ExtO1: O(log n) extension access by `TypeId` (n = registered types, typically < 20).
//! - I-Core-VTableClone: VTable provides `clone_box` for COW rollback.
//! - I-Core-Pure: Kernel never produces side effects.
//! - I-Core-NoPanic: `transition()` returns `Vec<Effect>`, never panics.
//!
//! Refs: docs/SPECS.md §Book I

// Allow proc-macro generated code to reference `::brioche_core` from
// inside the crate itself.
extern crate self as brioche_core;

pub mod engine;
pub mod extension;
pub mod plugin;
pub mod types;

// Re-export the derive macro so users can `use brioche_core::BriocheExtensionType;`
// and apply `#[derive(BriocheExtensionType)]` with a single import.
pub use brioche_macro::BriocheExtensionType;
pub use engine::{BriocheEngine, BriocheEngineBuilder, Missing, Present, UnifiedRoutingTable};
pub use extension::{
    BriocheExtensionType, CloneBoxFn, DefaultConstructFn, DeserializeFn, ExtVTable,
    ExtensionStorage, SerializeFn, SnapshotStrategy, WeightFn,
};
pub use plugin::{
    BriochePlugin, ConsistencyVerifier, CoreTypes, CowBudgetPolicy, CycleRollbackPolicy,
    DecisionAggregator, EpochInterceptor, GovernanceFailoverHandler, HookEffectConstraint,
    PluginCapabilities, SignalDrainOrder, SubRoutineHandler, SubRoutineHydrator,
    SubRoutineLifecycleGuard,
};
// Re-export dependencies so that proc-macro generated code and users
// can reference them through brioche_core without adding them to
// their own Cargo.toml.
pub use postcard;
pub use serde;
pub use types::{
    ActiveToolCall, AgentState, AgentStateTag, AsyncTaskResult, BriocheError, ChatMessage,
    DEFAULT_TOOL_TIMEOUT_MS, Effect, EffectBit, EngineInput, EpochAction, EpochState, ErrorCode,
    ErrorDetail, ExecutionPath, GovernanceNotification, HistoryEdit, MAX_INLINE_CHUNK, PluginError,
    PluginResult, PluginSource, PolicyDecision, RollbackEvent, RollbackEventLog, Session,
    SessionRegistry, SessionSnapshot, SignalBuffer, SignalDrainBatch, StreamAction, StreamEvent,
    StreamToolAccumulator, SubRoutineHandle, SupersededTransitionTrace,
    SupersededTransitionTraceLog, SystemSignal, TaskId, ToolCallDescriptor, ToolOutcome,
    ToolResultDTO, ToolStatus, TransitionTrace, TransitionTraceLog, TruncatedToolResult, UiWidget,
    effect_to_bitmask, seal, seal_single, tool_outcome_to_string,
};
