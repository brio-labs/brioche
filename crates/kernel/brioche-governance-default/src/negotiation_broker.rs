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
    /// Phase courante (0–2).
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
pub struct NegotiationBroker {
    max_phases: u8,
}

impl NegotiationBroker {
    /// Creates a broker with 3 max phases.
    pub fn new() -> Self {
        Self { max_phases: 3 }
    }

    /// Creates a broker with a custom number of phases.
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
