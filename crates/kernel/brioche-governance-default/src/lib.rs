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
//! - `ToolExecutionTracker`: Telemetry observer for tool calls.
//! - `GovernanceProfile`: Predefined profiles (Permissive / Standard / Strict).
//! - `QuarantineManager`, `RecoveryPolicy`, `DepthGuard`: Safety plugins.
//! - `ToolTimeoutPolicy`, `SubRoutineTimeoutPolicy`: Timeout guards.
//! - `AdaptiveUndoFrameGuard`, `TieredUndoFrameGuard`: Advanced COW rollback.
//! - `NegotiationBroker`, `TreeDecisionAggregator`: Alternative aggregators.
//!
//! ## Invariants upheld
//! - I-Gov-TraitAtomic: Each plugin implements exactly one trait.
//! - I-Gov-NoCoreMutation: Plugins only mutate their own `ExtensionStorage` state.
//!
//! Refs: SPECS.md §Book II

#![deny(clippy::unwrap_used, clippy::expect_used)]

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

pub mod adaptive_undo_frame_guard;
pub mod aggregators;
pub mod compatibility_matrix;
pub mod depth_guard;
pub mod guards;
pub mod historical_cow_budget_policy;
pub mod json_argument_accumulator;
pub mod negotiation_broker;
pub mod noop_traits;
pub mod quarantine_manager;
pub mod recovery_policy;
pub mod rollback_telemetry_emitter;
pub mod subroutines;
pub mod telemetry;
pub mod tiered_undo_frame_guard;
pub mod timeouts;
pub mod tool_execution_tracker;
pub mod tool_result_formatter;
pub mod tree_decision_aggregator;
pub mod undo_frame_guard;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use adaptive_undo_frame_guard::AdaptiveUndoFrameGuard;
pub use aggregators::{FastHookEffectConstraint, LexicographicDecisionAggregator};
pub use compatibility_matrix::{
    CompatibilityEntry, CompatibilityLevel, GovernanceCompatibilityMatrix,
};
pub use depth_guard::{DepthGuard, DepthState};
pub use guards::{EpochGuard, StateConsistencyGuard, SystemFailoverGuard};
pub use historical_cow_budget_policy::HistoricalCowBudgetPolicy;
pub use json_argument_accumulator::{JsonArgumentAccumulator, JsonArgumentAccumulatorState};
pub use negotiation_broker::{NegotiationBroker, NegotiationState};
pub use noop_traits::{
    NoopCowBudgetPolicy, NoopCycleRollbackPolicy, NoopGovernanceFailoverHandler,
    NoopHookEffectConstraint, PermissiveHookEffectConstraint,
};
pub use quarantine_manager::{PluginFaultKey, QuarantineManager, QuarantineState};
pub use recovery_policy::{RecoveryPolicy, RecoveryState};
pub use rollback_telemetry_emitter::{RollbackTelemetryEmitter, RollbackTelemetryState};
pub use subroutines::{SubRoutineCleanupGuard, SubRoutineOrchestrator};
pub use telemetry::{
    ToolCallDetector, ToolCallDetectorState, TransitionConflictLogger, TransitionConflictState,
};
pub use tiered_undo_frame_guard::TieredUndoFrameGuard;
pub use timeouts::{SubRoutineTimeoutPolicy, SubRoutineTimerState, ToolTimeoutPolicy};
pub use tool_execution_tracker::{ToolExecutionTelemetry, ToolExecutionTracker};
pub use tool_result_formatter::{ToolResultFormatter, ToolResultFormatterState};
pub use tree_decision_aggregator::{
    DecisionCondition, DecisionNode, DecisionTreeState, TreeDecisionAggregator,
};
pub use undo_frame_guard::UndoFrameGuard;

// GovernanceProfile is re-exported at crate root for one-line bootstrap.
mod profile;
pub use profile::{BriocheEngineBuilderExt, GovernanceProfile};
