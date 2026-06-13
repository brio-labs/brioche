//! Book I — The Core Book: Transition finalization.
//!
//! The last phase of every `transition()` call. Applies lifecycle guards,
//! effect validation, consistency checks, and position guarantees.
//!
//! ## Invariants upheld
//! - I-Core-NoPanic: Anomalies produce `Effect::Error`, not panics.
//! - I-Gov-SubRoutineLifecycle-Guard: Cleanup on sub-routine exit.
//! - I-Gov-Rebuild-Barrier: `RebuildRoutes` is always last.
//!
//! Refs: docs/SPECS.md §4.4

use super::{BriocheEngine, PreTransitionState};
use crate::{Effect, ErrorCode, ErrorDetail, Session, effect_to_bitmask};

impl BriocheEngine {
    /// Finalize a transition: apply lifecycle guards, consistency checks,
    /// and position guarantees.
    ///
    /// This is the last phase of every `transition()` call. It evaluates
    /// optional and mandatory governance traits in fixed order and may
    /// mutate or truncate the effect vector.
    ///
    /// # Complexity
    /// O(e) where e = number of effects.
    ///
    /// Refs: I-Core-NoPanic, I-Gov-SubRoutineLifecycle-Guard,
    /// I-Core-HookEffect-O1, I-Gov-Rebuild-Barrier, I-Gov-Failover
    /// # Panics
    /// Never panics.
    pub(crate) fn finalize_transition(
        &mut self,
        session: &mut Session,
        pre: PreTransitionState,
        effects: &mut Vec<Effect>,
    ) {
        self.apply_subroutine_lifecycle_guard(session, pre, effects);
        self.validate_hook_effects(effects);
        Self::ensure_rebuildroutes_last(effects);
        self.apply_consistency_check(session, effects);
        self.apply_governance_failover(session, effects);
    }

    /// Apply `SubRoutineLifecycleGuard` if the transition exits a sub-routine.
    ///
    /// Detected by comparing the pre-transition `SubRoutine` state with the
    /// post-transition state. If the sub-routine is no longer active, the
    /// guard's `on_exit` hook runs.
    ///
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
    fn apply_subroutine_lifecycle_guard(
        &mut self,
        session: &mut Session,
        pre: PreTransitionState,
        effects: &mut Vec<Effect>,
    ) {
        if !pre.was_subroutine {
            return;
        }
        if matches!(session.state, crate::AgentState::SubRoutine(_)) {
            return;
        }
        let Some(ref guard) = self.governance.subroutine_lifecycle_guard else {
            return;
        };
        let Some(handle) = pre.handle else {
            return;
        };

        match guard.on_exit(handle, session, &mut self.routines.registry) {
            Ok(guard_effects) => effects.extend(guard_effects),
            Err(err) => {
                effects.push(Self::plugin_fault("subroutine_lifecycle_guard", err));
            }
        }
    }

    /// Validate effects against `HookEffectConstraint`.
    ///
    /// `Effect::Error`, `Effect::PluginFault`, and `Effect::SystemIdle` are
    /// unconditionally allowed regardless of constraint masks.
    ///
    /// Refs: I-Core-HookEffect-O1
    fn validate_hook_effects(&self, effects: &mut [Effect]) {
        let Some(ref constraint) = self.governance.hook_effect_constraint else {
            return;
        };

        for effect in effects.iter_mut() {
            if matches!(
                effect,
                Effect::Error { .. } | Effect::PluginFault { .. } | Effect::SystemIdle
            ) {
                continue;
            }
            let mask = effect_to_bitmask(effect);
            if constraint.is_allowed_fast(0, mask) {
                continue;
            }
            // Fallback: format discriminant. Cold path; allocates.
            let variant = format!("{:?}", std::mem::discriminant(effect));
            if !constraint.is_allowed_fallback("transition", &variant) {
                *effect = Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    detail: ErrorDetail::EffectNotAllowed {
                        hook: "transition".to_string(),
                        effect_variant: "Effect::Error".to_string(),
                    },
                };
            }
        }
    }

    /// Run `ConsistencyVerifier` unless `RebuildRoutes` is present.
    ///
    /// A rebuild is a transactional barrier; consistency checks are skipped
    /// because the routing table change may intentionally leave transient
    /// inconsistent states.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    fn apply_consistency_check(&self, session: &mut Session, effects: &mut Vec<Effect>) {
        let Some(ref verifier) = self.governance.consistency_verifier else {
            return;
        };

        if effects.iter().any(|e| matches!(e, Effect::RebuildRoutes)) {
            return;
        }

        match verifier.verify_consistency(session) {
            Ok(Some(verifier_effects)) => {
                effects.extend(verifier_effects);
            }
            Ok(None) => {}
            Err(err) => {
                effects.push(Self::plugin_fault("consistency_verifier", err));
            }
        }
    }

    /// Run `GovernanceFailoverHandler` on `PluginFault` effects.
    ///
    /// If the handler produces replacement effects, they replace the fault.
    /// If the handler itself errors, the original fault is preserved.
    /// Skipped when `RebuildRoutes` is present (transactional barrier).
    ///
    /// Refs: I-Gov-Failover
    fn apply_governance_failover(&self, session: &mut Session, effects: &mut Vec<Effect>) {
        let Some(ref handler) = self.governance.governance_failover_handler else {
            return;
        };
        if effects.iter().any(|e| matches!(e, Effect::RebuildRoutes)) {
            return;
        }

        let mut replacement_effects = Vec::new();
        let mut has_fault = false;
        for effect in effects.iter() {
            if let Effect::PluginFault { .. } = effect {
                has_fault = true;
                match handler.handle_failure(session, effect) {
                    Ok(Some(failover)) => {
                        replacement_effects.extend(failover);
                    }
                    Ok(None) | Err(_) => {
                        replacement_effects.push(effect.clone());
                    }
                }
            } else {
                replacement_effects.push(effect.clone());
            }
        }
        if has_fault {
            *effects = replacement_effects;
        }
    }
}
