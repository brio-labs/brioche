//! # Brioche Governance Default — Book II (Implementations)
//!
//! Reference implementations of governance traits defined in
//! `brioche-governance`. These are the default plugins shipped with
//! Brioche.
//!
//! ## Public interface
//! - `EpochGuard`: Default `EpochInterceptor` implementation.
//! - `LexicographicDecisionAggregator`: Default `DecisionAggregator`.
//! - `SubRoutineCleanupGuard`: Default `SubRoutineLifecycleGuard`.
//! - `StateConsistencyGuard`: Default `ConsistencyVerifier`.
//! - `FastHookEffectConstraint`: Default `HookEffectConstraint`.
//! - `NoopCycleRollbackPolicy`: Null `CycleRollbackPolicy`.
//! - `SystemFailoverGuard`: Default `GovernanceFailoverHandler`.
//! - `SubRoutineOrchestrator`: Default `SubRoutineHandler`.
//! - `GovernanceProfile`: Predefined profiles (Permissive / Standard / Strict).
//! - `error_safety`: `QuarantineManager` and `RecoveryPolicy`.
//! - `input_guards`: `DepthGuard` and `JsonArgumentAccumulator`.
//! - `tool_pipeline`: `ToolResultFormatter` and `ToolExecutionTracker`.
//! - `timeouts`: `ToolTimeoutPolicy` and `SubRoutineTimeoutPolicy`.
//! - `rollback`: `UndoFrameGuard`, `TieredUndoFrameGuard`, `AdaptiveUndoFrameGuard`,
//!   and `HistoricalCowBudgetPolicy`.
//! - `telemetry`: `ToolCallDetector`, `TransitionConflictLogger`, and
//!   `RollbackTelemetryEmitter`.
//! - `NegotiationBroker`, `TreeDecisionAggregator`: Alternative aggregators.
//!
//! ## Invariants upheld
//! - I-Gov-TraitAtomic: Each plugin implements exactly one trait.
//! - I-Gov-NoCoreMutation: Plugins only mutate their own `ExtensionStorage` state.
//!
//! Refs: docs/SPECS.md §Book II

#![deny(clippy::unwrap_used, clippy::expect_used)]

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

pub mod aggregators;
pub mod compatibility_matrix;
pub mod error_safety;
pub mod guards;
pub mod input_guards;
pub mod negotiation_broker;
pub mod noop_traits;
pub mod rollback;
pub mod subroutines;
pub mod telemetry;
pub mod timeouts;
pub mod tool_pipeline;
pub mod tree_decision_aggregator;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use aggregators::{FastHookEffectConstraint, LexicographicDecisionAggregator};
pub use compatibility_matrix::{
    CompatibilityEntry, CompatibilityLevel, GovernanceCompatibilityMatrix,
};
pub use error_safety::{
    PluginFaultKey, QuarantineManager, QuarantineState, RecoveryPolicy, RecoveryState,
};
pub use guards::{EpochGuard, StateConsistencyGuard, SystemFailoverGuard};
pub use input_guards::{
    DepthGuard, DepthState, JsonArgumentAccumulator, JsonArgumentAccumulatorState, calculate_depth,
};
pub use negotiation_broker::{NegotiationBroker, NegotiationState};
pub use noop_traits::{
    NoopCowBudgetPolicy, NoopCycleRollbackPolicy, NoopGovernanceFailoverHandler,
    NoopHookEffectConstraint, PermissiveHookEffectConstraint,
};
pub use rollback::{
    AdaptiveUndoFrameGuard, RollbackFrameRecord, TieredUndoFrameGuard, UndoFrameGuard,
};
pub use subroutines::{SubRoutineCleanupGuard, SubRoutineOrchestrator};
pub use telemetry::{RollbackTelemetryState, TelemetryPlugin, TransitionConflictState};
pub use timeouts::{SubRoutineTimeoutPolicy, SubRoutineTimerState, ToolTimeoutPolicy};
pub use tool_pipeline::{
    ToolExecutionTelemetry, ToolExecutionTracker, ToolResultFormatter, ToolResultFormatterState,
};
pub use tree_decision_aggregator::{
    DecisionCondition, DecisionNode, DecisionTreeState, TreeDecisionAggregator,
};

// GovernanceProfile is re-exported at crate root for one-line bootstrap.
mod profile;
pub use profile::{BriocheEngineBuilderExt, GovernanceProfile};

// ---------------------------------------------------------------------------
// Priority constants
// ---------------------------------------------------------------------------

/// Named evaluation priorities for governance-default plugins.
///
/// Lower values run earlier. Ties are broken lexicographically by plugin
/// `name`. Using named constants makes the cross-plugin ordering policy
/// explicit and searchable.
///
/// Refs: I-Core-PluginOrder
pub struct Priority;

impl Priority {
    /// Very early `on_error` handler — quarantine before recovery.
    pub const QUARANTINE: i16 = -100;
    /// Early input interceptor — recover from cascading failures.
    pub const RECOVERY: i16 = -50;
    /// Input guard — enforce sub-routine depth limits.
    pub const DEPTH_GUARD: i16 = -40;
    /// Sub-routine timeout enforcement.
    pub const SUBROUTINE_TIMEOUT: i16 = -30;
    /// Tool timeout bounds before `ExecuteTools`.
    pub const TOOL_TIMEOUT: i16 = -10;
    /// Tool result formatting before other plugins inspect results.
    pub const TOOL_FORMATTER: i16 = 10;
    /// Stream-event argument accumulation.
    pub const ARGUMENT_ACCUMULATOR: i16 = 20;
    /// Late telemetry observer.
    pub const TELEMETRY: i16 = 100;
}
