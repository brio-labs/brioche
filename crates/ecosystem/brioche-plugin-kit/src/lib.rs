//! # Brioche Plugin Kit — Book IV
//!
//! SDK and developer tooling for third-party plugin authors.
//! Re-exports stable traits, types, and macros from Core and Governance.
//! Provides `PluginBuilder`, `MockEngine`, and test utilities.
//!
//! ## Public interface
//! - `brioche_plugin_kit` prelude: stable re-exports for plugin crates.
//! - `PluginBuilder`: Helper for constructing plugin registries.
//! - `MockEngine`: Test engine with all mandatory governance traits pre-wired.
//! - `#[brioche_plugin]`, `#[hook]`, `#[brioche_offload_task]`: Proc macros.
//!
//! ## Stability guarantee
//! Items exported from this crate follow semver. Internal crate APIs
//! (`brioche-core`, `brioche-governance`) may change without notice.
//!
//! Refs: SPECS.md §Book IV

#![deny(clippy::unwrap_used, clippy::expect_used)]

// Re-export proc macros from brioche-macro.
// Re-export core types (stable interface).
pub use brioche_core::{
    ActiveToolCall, AgentState, AgentStateTag, AsyncTaskResult, BriocheError, BriochePlugin,
    ChatMessage, ConsistencyVerifier, CowBudgetPolicy, CycleRollbackPolicy, DecisionAggregator,
    Effect, EffectBit, EngineInput, EpochAction, EpochInterceptor, EpochState, ErrorCode,
    ExecutionPath, ExtVTable, ExtensionStorage, GovernanceFailoverHandler, GovernanceNotification,
    HistoryEdit, HookEffectConstraint, MAX_INLINE_CHUNK, PluginCapabilities, PluginError,
    PluginResult, PolicyDecision, Session, SessionRegistry, SessionSnapshot, SignalBuffer,
    SignalDrainBatch, SignalDrainOrder, SnapshotStrategy, StreamAction, StreamEvent,
    StreamToolAccumulator, SubRoutineHandle, SubRoutineHandler, SubRoutineLifecycleGuard,
    SystemSignal, ToolCallDescriptor, ToolOutcome, ToolResultDTO, ToolStatus, TransitionTrace,
    TransitionTraceLog, effect_to_bitmask, seal,
};
// Re-export governance default implementations.
pub use brioche_governance_default::{
    AdaptiveUndoFrameGuard, BriocheEngineBuilderExt, CompatibilityEntry, CompatibilityLevel,
    DecisionCondition, DecisionNode, DecisionTreeState, DepthGuard, DepthState, EpochGuard,
    FastHookEffectConstraint, GovernanceCompatibilityMatrix, GovernanceProfile,
    HistoricalCowBudgetPolicy, JsonArgumentAccumulator, JsonArgumentAccumulatorState,
    LexicographicDecisionAggregator, NegotiationBroker, NegotiationState, NoopCowBudgetPolicy,
    NoopCycleRollbackPolicy, NoopGovernanceFailoverHandler, NoopHookEffectConstraint,
    PermissiveHookEffectConstraint, PluginFaultKey, QuarantineManager, QuarantineState,
    RecoveryPolicy, RecoveryState, RollbackTelemetryEmitter, RollbackTelemetryState,
    StateConsistencyGuard, SubRoutineCleanupGuard, SubRoutineOrchestrator, SubRoutineTimeoutPolicy,
    SubRoutineTimerState, SystemFailoverGuard, TieredUndoFrameGuard, ToolCallDetector,
    ToolCallDetectorState, ToolExecutionTelemetry, ToolExecutionTracker, ToolResultFormatter,
    ToolResultFormatterState, ToolTimeoutPolicy, ToolTimeoutState, TransitionConflictLogger,
    TreeDecisionAggregator, UndoFrameGuard,
};
// Also re-export BriocheExtensionType derive for convenience.
pub use brioche_macro::BriocheExtensionType;
pub use brioche_macro::{brioche_offload_task, brioche_plugin, hook};
// Re-export standard plugins.
pub use brioche_std::{
    AuditEntry, AuditLogger, AuditLoggerState, CircuitBreaker, CircuitBreakerState,
    ContextOptimizer, ContextOptimizerState, GcPolicy, GcPolicyState, PendingTaskInfo,
    PendingTaskManager, PendingTaskState, PendingTaskStatus, TokenTracker, TokenTrackerState,
    ToolTimeoutPolicy as StdToolTimeoutPolicy, ToolTimeoutState as StdToolTimeoutState,
};

pub mod builder;
pub mod mock_engine;

pub use builder::PluginBuilder;
pub use mock_engine::MockEngine;

/// Convenience prelude for plugin authors.
///
/// Import everything needed to write a basic Brioche plugin.
pub mod prelude {
    pub use crate::{
        BriocheError, BriocheExtensionType, BriochePlugin, ChatMessage, Effect, EngineInput,
        EpochState, ExtensionStorage, PluginCapabilities, PluginError, PluginResult,
        PolicyDecision, Session, SessionSnapshot, StreamAction, StreamEvent, ToolCallDescriptor,
        ToolResultDTO, brioche_offload_task, brioche_plugin, hook,
    };
}
