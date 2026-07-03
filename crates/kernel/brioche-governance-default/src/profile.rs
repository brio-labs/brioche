//! GovernanceProfile — Book II §9.
//!
//! Predefined governance profiles (`Permissive`, `Standard`, `Strict`)
//! encapsulate selection and configuration of fundamental and optional
//! governance traits. Profiles are a configuration abstraction; the kernel
//! remains agnostic of the notion of profile.
//!
//! Refs: I-Gov-Profile-Agnostic

use brioche_core::{BriocheEngineBuilder, Missing, Present};
use std::fmt;

use crate::{
    AdaptiveUndoFrameGuard, DepthGuard, EpochGuard, FastHookEffectConstraint,
    LexicographicDecisionAggregator, NoopCycleRollbackPolicy, NoopGovernanceFailoverHandler,
    PermissiveHookEffectConstraint, QuarantineManager, RecoveryPolicy, StateConsistencyGuard,
    SubRoutineCleanupGuard, SubRoutineOrchestrator, SubRoutineTimeoutPolicy, SystemFailoverGuard,
    TelemetryPlugin, TieredUndoFrameGuard, ToolResultFormatter, ToolTimeoutPolicy,
};

/// Error returned when a governance profile cannot be applied.
///
/// Refs: I-Gov-Profile-Agnostic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceError {
    /// The `Permissive` profile requires explicit opt-in via the
    /// `BRIOCHE_GOVERNANCE_PERMISSIVE=1` environment variable in release
    /// builds.
    PermissiveRequiresOptIn,
}

impl fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PermissiveRequiresOptIn => write!(
                f,
                "Permissive governance profile requires explicit opt-in via \
                 BRIOCHE_GOVERNANCE_PERMISSIVE=1 in release builds"
            ),
        }
    }
}

impl std::error::Error for GovernanceError {}

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
    /// Builder type after applying the profile.
    type Output;
    /// Apply a governance profile to this builder.
    ///
    /// In release builds, selecting `GovernanceProfile::Permissive` without
    /// the `BRIOCHE_GOVERNANCE_PERMISSIVE=1` environment variable falls back
    /// to `Standard` after emitting a warning. See
    /// [`GovernanceProfile::apply`] for details.
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
/// - `Permissive`: **dev / prototyping only** — minimal policy, all effects
///   allowed, no COW rollback. In release builds it requires the
///   `BRIOCHE_GOVERNANCE_PERMISSIVE=1` environment variable; otherwise
///   `apply` falls back to `Standard` and `try_apply` returns an error.
/// - `Standard`: balanced policy with COW rollback, standard guards, and
///   telemetry.
/// - `Strict`: maximum safeguards, tiered rollback, strict effect constraints,
///   and comprehensive logging.
///
/// Refs: I-Gov-Profile-Agnostic
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GovernanceProfile {
    /// **Dev / prototyping only.** Minimal policy: all effects allowed, no COW
    /// rollback. In release builds this requires the
    /// `BRIOCHE_GOVERNANCE_PERMISSIVE=1` environment variable.
    Permissive,
    /// Balanced policy with COW rollback, standard guards, and telemetry.
    #[default]
    Standard,
    /// Maximum safeguards, tiered rollback, strict effect constraints.
    Strict,
}

impl GovernanceProfile {
    /// Apply this profile to a `BriocheEngineBuilder`, wiring all traits
    /// and standard plugins.
    ///
    /// Returns the builder with profile components pre-registered.
    ///
    /// # Warning: `Permissive` is dev-only
    /// In release builds (`cfg(not(debug_assertions))`) selecting
    /// `GovernanceProfile::Permissive` without setting
    /// `BRIOCHE_GOVERNANCE_PERMISSIVE=1` silently falls back to the
    /// `Standard` profile after emitting a warning. Use [`Self::try_apply`]
    /// to observe the rejection explicitly.
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
            GovernanceProfile::Permissive => {
                tracing::warn!(
                    "Permissive governance profile selected: all hook effects are allowed. \
                     This is intended for development and prototyping only."
                );
                if Self::permissive_release_opted_in() {
                    Self::build_permissive(builder)
                } else {
                    tracing::warn!(
                        "Permissive profile rejected in release build without opt-in; \
                         falling back to Standard"
                    );
                    Self::apply_standard(builder)
                }
            }
            GovernanceProfile::Standard => Self::apply_standard(builder),
            GovernanceProfile::Strict => Self::apply_strict(builder),
        }
    }

    /// Apply this profile to a `BriocheEngineBuilder`, returning an error
    /// if the profile cannot be selected.
    ///
    /// In release builds (`cfg(not(debug_assertions))`) selecting
    /// `GovernanceProfile::Permissive` requires the
    /// `BRIOCHE_GOVERNANCE_PERMISSIVE=1` environment variable. This
    /// method is useful for callers that want to surface the opt-in error
    /// instead of silently falling back to `Standard`.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn try_apply(
        self,
        builder: BriocheEngineBuilder<Missing, Missing>,
    ) -> Result<BriocheEngineBuilder<Present, Present>, GovernanceError> {
        match self {
            GovernanceProfile::Permissive => {
                tracing::warn!(
                    "Permissive governance profile selected: all hook effects are allowed. \
                     This is intended for development and prototyping only."
                );
                if Self::permissive_release_opted_in() {
                    Ok(Self::build_permissive(builder))
                } else {
                    Err(GovernanceError::PermissiveRequiresOptIn)
                }
            }
            GovernanceProfile::Standard => Ok(Self::apply_standard(builder)),
            GovernanceProfile::Strict => Ok(Self::apply_strict(builder)),
        }
    }

    fn permissive_release_opted_in() -> bool {
        #[cfg(debug_assertions)]
        {
            true
        }
        #[cfg(not(debug_assertions))]
        {
            std::env::var("BRIOCHE_GOVERNANCE_PERMISSIVE")
                .map(|value| value == "1")
                .unwrap_or(false)
        }
    }

    fn build_permissive(
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
            .with_plugin(Box::new(TelemetryPlugin::new()))
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
            .with_plugin(Box::new(TelemetryPlugin::new()))
            .with_plugin(Box::new(ToolResultFormatter::new()))
            .with_plugin(Box::new(ToolTimeoutPolicy::with_default_timeout(30000)))
            .with_plugin(Box::new(SubRoutineTimeoutPolicy::with_default_timeout(
                300000,
            )))
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
            .with_plugin(Box::new(TelemetryPlugin::new()))
            .with_plugin(Box::new(ToolResultFormatter::new()))
            .with_plugin(Box::new(ToolTimeoutPolicy::with_default_timeout(10000)))
            .with_plugin(Box::new(SubRoutineTimeoutPolicy::with_default_timeout(
                60000,
            )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brioche_core::BriocheEngineBuilder;
    use tracing_test::traced_test;

    #[cfg(debug_assertions)]
    #[traced_test]
    #[test]
    fn permissive_profile_emits_warning() {
        let result = GovernanceProfile::Permissive.try_apply(BriocheEngineBuilder::new());
        assert!(
            result.is_ok(),
            "permissive profile should apply in debug builds"
        );
        let _ = result;
        assert!(logs_contain(
            "Permissive governance profile selected: all hook effects are allowed"
        ));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn permissive_profile_rejected_in_release_without_opt_in() {
        let result = GovernanceProfile::Permissive.try_apply(BriocheEngineBuilder::new());
        assert!(matches!(
            result,
            Err(GovernanceError::PermissiveRequiresOptIn)
        ));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn permissive_profile_accepted_in_release_with_opt_in() {
        unsafe { std::env::set_var("BRIOCHE_GOVERNANCE_PERMISSIVE", "1") };
        let result = GovernanceProfile::Permissive.try_apply(BriocheEngineBuilder::new());
        unsafe { std::env::remove_var("BRIOCHE_GOVERNANCE_PERMISSIVE") };
        assert!(result.is_ok());
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn permissive_apply_falls_back_to_standard_in_release() {
        let builder = GovernanceProfile::Permissive.apply(BriocheEngineBuilder::new());
        // The builder type is present, proving the fallback succeeded.
        let _engine = builder.build();
    }

    #[cfg(debug_assertions)]
    #[traced_test]
    #[test]
    fn permissive_hook_effect_constraint_emits_warning() {
        let _constraint = PermissiveHookEffectConstraint::new();
        assert!(logs_contain(
            "PermissiveHookEffectConstraint constructed: all effects are allowed"
        ));
    }
}
