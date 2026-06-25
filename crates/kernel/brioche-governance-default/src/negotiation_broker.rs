//! NegotiationBroker — Book II §5.22.
//!
//! Multi-turn negotiation via `ExtensionStorage` (max 3 phases).
//!
//! Instead of a single `before_prediction` decision, the broker allows
//! plugins to negotiate across up to 3 rounds before finalizing.
//!
//! Refs: I-Gov-Decision-Required

use brioche_core::{DecisionAggregator, Effect, ExtensionStorage, PluginResult, PolicyDecision};

/// Ongoing negotiation state.
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with number of phase decisions
/// (bounded by `max_phases`, typically 2264 3). One `Vec` + two scalars.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
#[brioche(critical_state)]
pub struct NegotiationState {
    /// Current phase (0–2).
    pub current_phase: u8,
    /// Decisions accumulated per phase.
    #[brioche(deterministic_order)]
    pub phase_decisions: Vec<PolicyDecision>,
    /// Negotiation settled?
    pub settled: bool,
}

/// Multi-phase negotiation broker.
///
/// On `aggregate_decisions`, performs up to 3 negotiation phases
/// before returning a final decision.
///
/// Refs: I-Gov-Decision-Required
pub struct NegotiationBroker {
    max_phases: u8,
}

impl NegotiationBroker {
    /// Creates a broker with 3 max phases.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self { max_phases: 3 }
    }

    /// Creates a broker with a custom number of phases.
    /// Refs: I-Gov-TraitAtomic
    pub fn with_max_phases(max_phases: u8) -> Self {
        Self { max_phases }
    }
}

impl Default for NegotiationBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl DecisionAggregator for NegotiationBroker {
    fn aggregate_decisions(
        &self,
        decisions: Vec<PolicyDecision>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let state = ext.get_or_insert_default::<NegotiationState>();

        // Accumulate this phase's decisions.
        for d in &decisions {
            state.phase_decisions.push(d.clone());
        }

        state.current_phase += 1;

        // If we've reached max phases or all decisions agree, settle.
        let all_allow = decisions.iter().all(|d| matches!(d, PolicyDecision::Allow));
        let any_block = decisions
            .iter()
            .any(|d| matches!(d, PolicyDecision::Block { .. }));
        let any_override = decisions
            .iter()
            .any(|d| matches!(d, PolicyDecision::OverrideTransition(_)));

        if state.current_phase >= self.max_phases || all_allow || any_block || any_override {
            state.settled = true;
        }

        if state.settled {
            // Final aggregation using lexicographic rules.
            let mut edits = Vec::new();
            let mut effects = Vec::new();

            for decision in &state.phase_decisions {
                match decision {
                    PolicyDecision::Allow => {}
                    PolicyDecision::Block { reason } => {
                        return Ok(PolicyDecision::Block {
                            reason: reason.clone(),
                        });
                    }
                    PolicyDecision::MutateHistory(e) => {
                        edits.extend(e.clone());
                    }
                    PolicyDecision::RequestEffect(eff) => {
                        effects.push(eff.clone());
                    }
                    PolicyDecision::OverrideTransition(ov) => {
                        return Ok(PolicyDecision::OverrideTransition(ov.clone()));
                    }
                    _ => {}
                }
            }

            // Reset state for next negotiation cycle.
            state.current_phase = 0;
            state.phase_decisions.clear();
            state.settled = false;

            if !edits.is_empty() {
                Ok(PolicyDecision::MutateHistory(edits))
            } else if !effects.is_empty() {
                Ok(PolicyDecision::RequestEffect(effects.remove(0)))
            } else {
                Ok(PolicyDecision::Allow)
            }
        } else {
            // Not settled yet — request another prediction round.
            Ok(PolicyDecision::RequestEffect(Effect::CallLlmNetwork))
        }
    }
}

#[cfg(test)]
mod tests {
    use brioche_core::{DecisionAggregator, Effect, ExtensionStorage, HistoryEdit, PolicyDecision};

    use super::{NegotiationBroker, NegotiationState};

    #[test]
    fn negotiation_settles_on_allow() {
        let broker = NegotiationBroker::new();
        let mut ext = ExtensionStorage::new();

        let decision = broker.aggregate_decisions(vec![PolicyDecision::Allow], &mut ext);
        assert!(matches!(decision, Ok(PolicyDecision::Allow)));
    }

    #[test]
    fn negotiation_settles_on_block() {
        let broker = NegotiationBroker::new();
        let mut ext = ExtensionStorage::new();

        let decisions = vec![
            PolicyDecision::Allow,
            PolicyDecision::Block {
                reason: "stop".into(),
            },
        ];

        let decision = broker.aggregate_decisions(decisions, &mut ext);
        assert!(
            matches!(decision, Ok(PolicyDecision::Block { ref reason }) if reason == "stop"),
            "expected Block decision"
        );
    }

    #[test]
    fn negotiation_settles_on_override() {
        let broker = NegotiationBroker::new();
        let mut ext = ExtensionStorage::new();

        let override_effects = vec![Effect::SystemIdle];
        let decisions = vec![
            PolicyDecision::Allow,
            PolicyDecision::OverrideTransition(override_effects.clone()),
        ];

        let decision = broker.aggregate_decisions(decisions, &mut ext);
        assert!(
            matches!(decision, Ok(PolicyDecision::OverrideTransition(ref effects)) if effects == &override_effects),
            "expected OverrideTransition"
        );
    }

    #[test]
    fn negotiation_requests_another_round_when_unsettled() {
        let broker = NegotiationBroker::with_max_phases(3);
        let mut ext = ExtensionStorage::new();

        let decisions = vec![
            PolicyDecision::RequestEffect(Effect::CallLlmNetwork),
            PolicyDecision::RequestEffect(Effect::CallLlmNetwork),
        ];

        let decision = broker.aggregate_decisions(decisions, &mut ext);
        assert!(
            matches!(
                decision,
                Ok(PolicyDecision::RequestEffect(Effect::CallLlmNetwork))
            ),
            "expected another prediction round"
        );

        let state = ext.get_or_insert_default::<NegotiationState>();
        assert_eq!(state.current_phase, 1);
        assert_eq!(state.phase_decisions.len(), 2);
    }

    #[test]
    fn negotiation_resets_state_after_settlement() {
        let broker = NegotiationBroker::with_max_phases(2);
        let mut ext = ExtensionStorage::new();

        let first = broker.aggregate_decisions(vec![PolicyDecision::Allow], &mut ext);
        assert!(matches!(first, Ok(PolicyDecision::Allow)));

        let state = ext.get_or_insert_default::<NegotiationState>();
        assert_eq!(state.current_phase, 0);
        assert!(state.phase_decisions.is_empty());
        assert!(!state.settled);
    }

    #[test]
    fn negotiation_settles_after_max_phases() {
        let broker = NegotiationBroker::with_max_phases(1);
        let mut ext = ExtensionStorage::new();

        let decision = broker.aggregate_decisions(vec![PolicyDecision::Allow], &mut ext);
        assert!(matches!(decision, Ok(PolicyDecision::Allow)));
    }

    #[test]
    fn negotiation_collects_mutate_history_effects() {
        let broker = NegotiationBroker::new();
        let mut ext = ExtensionStorage::new();

        let edits = vec![HistoryEdit::Truncate { keep_last: 1 }];

        // MutateHistory does not trigger immediate settlement. Advance through
        // an unresolved phase so the next phase reaches all_allow and settles.
        let _ = broker.aggregate_decisions(
            vec![
                PolicyDecision::Allow,
                PolicyDecision::MutateHistory(edits.clone()),
            ],
            &mut ext,
        );
        let decision = broker.aggregate_decisions(vec![PolicyDecision::Allow], &mut ext);

        assert!(
            matches!(decision, Ok(PolicyDecision::MutateHistory(ref collected)) if collected == &edits),
            "expected collected history edits"
        );
    }
}
