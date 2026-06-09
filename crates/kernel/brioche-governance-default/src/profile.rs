//! GovernanceProfile — Book II §9.
//!
//! Predefined governance profiles (`Permissive`, `Standard`, `Strict`)
//! encapsulate selection and configuration of fundamental and optional
//! governance traits. Profiles are a configuration abstraction; the kernel
//! remains agnostic of the notion of profile.
//!
//! Refs: I-Gov-Profile-Agnostic

use brioche_core::{BriocheEngineBuilder, Missing, Present};

use crate::{
    AdaptiveUndoFrameGuard, DepthGuard, EpochGuard, FastHookEffectConstraint,
    LexicographicDecisionAggregator, NoopCycleRollbackPolicy, NoopGovernanceFailoverHandler,
    PermissiveHookEffectConstraint, QuarantineManager, RecoveryPolicy, RollbackTelemetryEmitter,
    StateConsistencyGuard, SubRoutineCleanupGuard, SubRoutineOrchestrator, SubRoutineTimeoutPolicy,
    SystemFailoverGuard, TieredUndoFrameGuard, ToolCallDetector, ToolExecutionTracker,
    ToolResultFormatter, ToolTimeoutPolicy, TransitionConflictLogger,
};

/// Extension trait providing `with_profile` on `BriocheEngineBuilder`.
///
/// This trait is defined in `brioche-governance-default` to avoid a
/// circular dependency (the kernel cannot depend on its default
/// implementations crate).
///
/// `with_profile` changes the builder's type from
/// `BriocheEngineBuilder<Missing, Missing>` to
/// `BriocheEngineBuilder<Present, Present>` because every profile
/// injects both mandatory traits.
///
/// Refs: I-Gov-Profile-Agnostic
pub trait BriocheEngineBuilderExt {
    type Output;
    /// Apply a governance profile to this builder.
    fn with_profile(self, profile: GovernanceProfile) -> Self::Output;
}

impl BriocheEngineBuilderExt for BriocheEngineBuilder<Missing, Missing> {
    type Output = BriocheEngineBuilder<Present, Present>;

    fn with_profile(self, profile: GovernanceProfile) -> Self::Output {
        profile.apply(self)
    }
}

/// Predefined governance profile.
///
/// A profile is a one-line bootstrap that wires all governance traits
/// and standard plugins into a `BriocheEngineBuilder`.
///
/// # Variants
/// - `Permissive`: minimal policy, all effects allowed, no COW rollback.
/// - `Standard`: balanced policy with COW rollback, standard guards, and
///   telemetry.
/// - `Strict`: maximum safeguards, tiered rollback, strict effect constraints,
///   and comprehensive logging.
///
/// Refs: I-Gov-Profile-Agnostic
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GovernanceProfile {
    Permissive,
    #[default]
    Standard,
    Strict,
}

impl GovernanceProfile {
    /// Apply this profile to a `BriocheEngineBuilder`, wiring all traits
    /// and standard plugins.
    ///
    /// Returns the builder with profile components pre-registered.
    ///
    /// # Example
    /// ```
    /// use brioche_core::BriocheEngineBuilder;
    /// use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
    ///
    /// let engine = BriocheEngineBuilder::new()
    ///     .with_profile(GovernanceProfile::Standard)
    ///     .build();
    /// ```
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn apply(
        self,
        builder: BriocheEngineBuilder<Missing, Missing>,
    ) -> BriocheEngineBuilder<Present, Present> {
        match self {
            GovernanceProfile::Permissive => Self::apply_permissive(builder),
            GovernanceProfile::Standard => Self::apply_standard(builder),
            GovernanceProfile::Strict => Self::apply_strict(builder),
        }
    }

    fn apply_permissive(
        builder: BriocheEngineBuilder<Missing, Missing>,
    ) -> BriocheEngineBuilder<Present, Present> {
        builder
            .with_epoch_interceptor(Box::new(EpochGuard))
            .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
            .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
            .with_subroutine_handler(Box::new(SubRoutineOrchestrator::new()))
            .with_consistency_verifier(Box::new(StateConsistencyGuard::new()))
            .with_hook_effect_constraint(Box::new(PermissiveHookEffectConstraint::new()))
            .with_cycle_rollback_policy(Box::new(NoopCycleRollbackPolicy))
            .with_governance_failover_handler(Box::new(NoopGovernanceFailoverHandler))
            .with_plugin(Box::new(ToolCallDetector::new()))
            .with_plugin(Box::new(ToolResultFormatter::new()))
            .with_plugin(Box::new(ToolExecutionTracker::new()))
    }

    fn apply_standard(
        builder: BriocheEngineBuilder<Missing, Missing>,
    ) -> BriocheEngineBuilder<Present, Present> {
        builder
            .with_epoch_interceptor(Box::new(EpochGuard))
            .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
            .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
            .with_subroutine_handler(Box::new(SubRoutineOrchestrator::new()))
            .with_consistency_verifier(Box::new(StateConsistencyGuard::new()))
            .with_hook_effect_constraint(Box::new(FastHookEffectConstraint::standard()))
            .with_cycle_rollback_policy(Box::new(AdaptiveUndoFrameGuard::new()))
            .with_governance_failover_handler(Box::new(SystemFailoverGuard::new()))
            .with_plugin(Box::new(QuarantineManager::new()))
            .with_plugin(Box::new(RecoveryPolicy::new()))
            .with_plugin(Box::new(DepthGuard::with_max_depth(10)))
            .with_plugin(Box::new(TransitionConflictLogger::new()))
            .with_plugin(Box::new(ToolCallDetector::new()))
            .with_plugin(Box::new(ToolResultFormatter::new()))
            .with_plugin(Box::new(ToolTimeoutPolicy::with_default_timeout(30000)))
            .with_plugin(Box::new(SubRoutineTimeoutPolicy::with_default_timeout(
                300000,
            )))
            .with_plugin(Box::new(ToolExecutionTracker::new()))
            .with_plugin(Box::new(RollbackTelemetryEmitter::new()))
    }

    fn apply_strict(
        builder: BriocheEngineBuilder<Missing, Missing>,
    ) -> BriocheEngineBuilder<Present, Present> {
        builder
            .with_epoch_interceptor(Box::new(EpochGuard))
            .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
            .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
            .with_subroutine_handler(Box::new(SubRoutineOrchestrator::new()))
            .with_consistency_verifier(Box::new(StateConsistencyGuard::new()))
            .with_hook_effect_constraint(Box::new(FastHookEffectConstraint::standard()))
            .with_cycle_rollback_policy(Box::new(TieredUndoFrameGuard::new()))
            .with_governance_failover_handler(Box::new(SystemFailoverGuard::new()))
            .with_plugin(Box::new(QuarantineManager::new()))
            .with_plugin(Box::new(RecoveryPolicy::new()))
            .with_plugin(Box::new(DepthGuard::with_max_depth(5)))
            .with_plugin(Box::new(TransitionConflictLogger::new()))
            .with_plugin(Box::new(ToolCallDetector::new()))
            .with_plugin(Box::new(ToolResultFormatter::new()))
            .with_plugin(Box::new(ToolTimeoutPolicy::with_default_timeout(10000)))
            .with_plugin(Box::new(SubRoutineTimeoutPolicy::with_default_timeout(
                60000,
            )))
            .with_plugin(Box::new(ToolExecutionTracker::new()))
            .with_plugin(Box::new(RollbackTelemetryEmitter::new()))
    }
}
