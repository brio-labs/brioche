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
//! - `CycleBudgetGuard`: Default `CycleBudgetPolicy`.
//!
//! ## Invariants upheld
//! - I-Gov-TraitAtomic: Each plugin implements exactly one trait.
//! - I-Gov-NoCoreMutation: Plugins only mutate their own `ExtensionStorage` state.
//!
//! Refs: SPECS.md §Book II

#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod cycle_budget_guard;
pub mod epoch_guard;
pub mod hook_effect_constraint;
pub mod noop_rollback_policy;
pub mod policy_aggregator;
pub mod state_consistency_guard;
pub mod subroutine_cleanup_guard;
pub mod subroutine_orchestrator;
pub mod system_failover_guard;

pub use cycle_budget_guard::{CycleBudgetGuard, CycleBudgetGuardState, CycleBudgetViolation};
pub use epoch_guard::EpochGuard;
pub use hook_effect_constraint::FastHookEffectConstraint;
pub use noop_rollback_policy::NoopCycleRollbackPolicy;
pub use policy_aggregator::LexicographicDecisionAggregator;
pub use state_consistency_guard::StateConsistencyGuard;
pub use subroutine_cleanup_guard::SubRoutineCleanupGuard;
pub use subroutine_orchestrator::SubRoutineOrchestrator;
pub use system_failover_guard::SystemFailoverGuard;
