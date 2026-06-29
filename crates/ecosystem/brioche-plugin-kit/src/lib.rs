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
//! (`brioche-core`, `brioche-governance-default`) may change without notice.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.2

#![deny(clippy::unwrap_used, clippy::expect_used)]

// Re-export proc macros from brioche-macro.
// Re-export core types (stable interface).
pub use brioche_core::{
    ActiveToolCall, AgentState, AgentStateTag, AsyncTaskResult, BriocheError, BriochePlugin,
    ChatMessage, ConsistencyVerifier, CowBudgetPolicy, CycleRollbackPolicy, DecisionAggregator,
    Effect, EffectBit, EngineInput, EpochAction, EpochInterceptor, EpochState, ErrorCode,
    ExecutionPath, ExtVTable, ExtensionStorage, GovernanceFailoverHandler, GovernanceNotification,
    HistoryEdit, HookEffectConstraint, MAX_INLINE_CHUNK, PluginCapabilities, PluginError,
    PluginResult, PluginSource, PolicyDecision, Session, SessionRegistry, SessionSnapshot,
    SignalBuffer, SignalDrainBatch, SignalDrainOrder, SnapshotStrategy, StreamAction, StreamEvent,
    StreamToolAccumulator, SubRoutineHandle, SubRoutineHandler, SubRoutineLifecycleGuard,
    SystemSignal, TaskId, ToolCallDescriptor, ToolOutcome, ToolResultDTO, ToolStatus,
    TransitionTrace, TransitionTraceLog, effect_to_bitmask, seal,
};
// Re-export governance default implementations.
pub use brioche_governance_default::{
    AdaptiveUndoFrameGuard, BriocheEngineBuilderExt, CompatibilityEntry, CompatibilityLevel,
    DecisionCondition, DecisionNode, DecisionTreeState, DepthGuard, DepthState, EpochGuard,
    FastHookEffectConstraint, GovernanceCompatibilityMatrix, GovernanceProfile,
    JsonArgumentAccumulator, JsonArgumentAccumulatorState, LexicographicDecisionAggregator,
    NegotiationBroker, NegotiationState, NoopCowBudgetPolicy, NoopCycleRollbackPolicy,
    NoopGovernanceFailoverHandler, NoopHookEffectConstraint, PermissiveHookEffectConstraint,
    PluginFaultKey, QuarantineManager, QuarantineState, RecoveryPolicy, RecoveryState,
    RollbackTelemetryState, StateConsistencyGuard, SubRoutineCleanupGuard, SubRoutineOrchestrator,
    SubRoutineTimeoutPolicy, SubRoutineTimerState, SystemFailoverGuard, TelemetryPlugin,
    TieredUndoFrameGuard, ToolExecutionTelemetry, ToolExecutionTracker, ToolResultFormatter,
    ToolResultFormatterState, ToolTimeoutPolicy, TransitionConflictState, TreeDecisionAggregator,
};
// Also re-export BriocheExtensionType derive for convenience.
pub use brioche_macro::BriocheExtensionType;
pub use brioche_macro::{brioche_offload_task, brioche_plugin, hook};
// Re-export standard plugins.
pub use brioche_std::{
    AuditEntry, AuditLogger, AuditLoggerState, CircuitBreaker, CircuitBreakerState,
    ContextOptimizer, ContextOptimizerState, GcPolicy, GcPolicyState, PendingTaskInfo,
    PendingTaskManager, PendingTaskState, PendingTaskStatus, TokenTracker, TokenTrackerState,
    ToolTimeoutPolicy as StdToolTimeoutPolicy,
};

pub mod builder;

// ---------------------------------------------------------------------------
// MockEngine (merged from mock_engine.rs)
// ---------------------------------------------------------------------------
use brioche_core::BriocheEngine;
pub use builder::PluginBuilder;

/// Pre-wired test engine with a fresh session.
///
/// Uses the `Permissive` governance profile so that policy plugins do
/// not interfere with the behavior under test. All mandatory governance
/// traits are injected with no-op or permissive implementations.
/// Refs: docs/SPECS.md §Book V
pub struct MockEngine {
    engine: BriocheEngine,
    session: Session,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    /// Create a new `MockEngine` with the `Permissive` profile.
    ///
    /// The session id is `"test"`.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn new() -> Self {
        let (engine, session) = PluginBuilder::permissive().build_with_session("test");
        Self { engine, session }
    }

    /// Create a new `MockEngine` with the `Standard` profile.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn standard() -> Self {
        let (engine, session) = PluginBuilder::standard().build_with_session("test");
        Self { engine, session }
    }

    /// Create a new `MockEngine` with the `Strict` profile.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn strict() -> Self {
        let (engine, session) = PluginBuilder::strict().build_with_session("test");
        Self { engine, session }
    }

    /// Execute one transition cycle.
    ///
    /// # Panics
    /// Never panics under normal operation. A panic indicates a bug in
    /// `brioche-core` (violating the NoPanic contract).
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn transition(&mut self, input: EngineInput) -> Vec<Effect> {
        self.engine.transition(&mut self.session, &input)
    }

    /// Mutable access to the underlying engine.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn engine(&mut self) -> &mut BriocheEngine {
        &mut self.engine
    }

    /// Mutable access to the session.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn session(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Immutable access to the session.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn session_ref(&self) -> &Session {
        &self.session
    }

    /// Consume the mock, returning the engine and session.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn into_parts(self) -> (BriocheEngine, Session) {
        (self.engine, self.session)
    }
}

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
