#![deny(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)]

//! Book I — The Core Book: Synchronous kernel and pure mechanisms.
//!
//! This crate upholds:
//! - I-Core-ExtensionType: Compile-time verified extension types via `BriocheExtensionType`.
//! - I-Core-ExtO1: O(log n) extension access by `TypeId` (n = registered types, typically < 20).
//! - I-Core-VTableClone: VTable provides `clone_box` for COW rollback.
//! - I-Core-Pure: Kernel never produces side effects.
//! - I-Core-NoPanic: `transition()` returns `Vec<Effect>`, never panics.
//!
//! Refs: SPECS.md §Book I

// Allow proc-macro generated code to reference `::brioche_core` from
// inside the crate itself.
extern crate self as brioche_core;

pub mod engine;
pub mod extension;
pub mod plugin;
pub mod types;

pub use engine::{BriocheEngine, BriocheEngineBuilder, UnifiedRoutingTable};
pub use extension::{
    BriocheExtensionType, CloneBoxFn, DefaultConstructFn, DeserializeFn, ExtVTable,
    ExtensionStorage, SerializeFn, SnapshotStrategy, WeightFn,
};
pub use plugin::{
    BriochePersistable, BriochePlugin, ConsistencyReport, ConsistencyVerifier, CowBudgetPolicy,
    CycleRollbackPolicy, DEFAULT_PLUGIN_PRIORITY, DecisionAggregator, EpochInterceptor,
    GovernanceFailoverHandler, HookEffectConstraint, PluginCapabilities, SignalDrainOrder,
    SubRoutineHandler, SubRoutineLifecycleGuard,
};
pub use types::{
    ActiveToolCall, AgentState, AgentStateTag, AsyncTaskResult, BriocheError, ChatMessage,
    DEFAULT_TOOL_TIMEOUT_MS, Effect, EffectBit, EngineInput, EpochAction, EpochState, ErrorCode,
    ExecutionPath, GovernanceNotification, HistoryEdit, INITIAL_GENERATION_ID, MAX_INLINE_CHUNK,
    PluginError, PluginResult, PolicyDecision, RollbackEvent, RollbackEventLog, Session,
    SessionRegistry, SessionSnapshot, SignalBuffer, SignalDrainBatch, StreamAction, StreamEvent,
    StreamToolAccumulator, SubRoutineHandle, SupersededTransitionTrace,
    SupersededTransitionTraceLog, SystemSignal, TRACE_LOG_CAPACITY, ToolCallDescriptor,
    ToolOutcome, ToolResultDTO, ToolStatus, TransitionTrace, TransitionTraceLog,
    TruncatedToolResult, UiWidget, effect_to_bitmask, seal,
};

// Re-export dependencies so that proc-macro generated code and users
// can reference them through brioche_core without adding them to
// their own Cargo.toml.
pub use postcard;
pub use serde;

// Re-export the derive macro so users can `use brioche_core::BriocheExtensionType;`
// and apply `#[derive(BriocheExtensionType)]` with a single import.
pub use brioche_macro::BriocheExtensionType;
