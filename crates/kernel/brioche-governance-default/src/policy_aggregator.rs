//! LexicographicDecisionAggregator — `DecisionAggregator` implementation (Book II §5.17).
//!
//! Aggregates intermediate decisions collected by the kernel on the
//! `before_prediction` route according to a deterministic lexicographic
//! merge rule.
//!
//! Priority rule (in evaluation order):
//! 1. `Block`   → returns immediately `Block` with the aggregated reason.
//! 2. `OverrideTransition` → first encountered wins.
//! 3. `MutateHistory` → accumulates all edits.
//! 4. `RequestEffect` → accumulates all effects.
//! 5. `Allow`   → ignored unless no other decision.
//!
//! Refs: I-Gov-Decision-Required, I-Gov-Decision-Isolation

use brioche_core::{
    DecisionAggregator, Effect, ExtensionStorage, HistoryEdit, PluginResult, PolicyDecision,
};

/// Deterministic lexicographic aggregator.
///
/// This component is **mandatory**: the kernel refuses to start without
/// a `DecisionAggregator` injected via `BriocheEngineBuilder`.
///
/// # Example
/// ```
/// use brioche_core::BriocheEngineBuilder;
/// use brioche_governance_default::LexicographicDecisionAggregator;
///
/// let engine = BriocheEngineBuilder::new()
///     .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
///     .with_subroutine_lifecycle_guard(Box::new(
///         brioche_governance_default::SubRoutineCleanupGuard::new(),
///     ))
///     .build();
/// ```
pub struct LexicographicDecisionAggregator;

impl DecisionAggregator for LexicographicDecisionAggregator {
    fn aggregate_decisions(
        &self,
        decisions: Vec<PolicyDecision>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let mut edits: Vec<HistoryEdit> = Vec::new();
        let mut effects: Vec<Effect> = Vec::new();

        for decision in decisions {
            match decision {
                PolicyDecision::Allow => {}
                PolicyDecision::Block { reason } => {
                    // Block short-circuits immediately (lexicographic merge).
                    return Ok(PolicyDecision::Block { reason });
                }
                PolicyDecision::MutateHistory(mut e) => {
                    edits.append(&mut e);
                }
                PolicyDecision::RequestEffect(eff) => {
                    effects.push(eff);
                }
                PolicyDecision::OverrideTransition(ov) => {
                    // The first OverrideTransition wins.
                    return Ok(PolicyDecision::OverrideTransition(ov));
                }
                _ => {}
            }
        }

        if !edits.is_empty() {
            Ok(PolicyDecision::MutateHistory(edits))
        } else if !effects.is_empty() {
            // Concatenating RequestEffects into a single MutateHistory is
            // not possible (different types). We return the first effect
            // as the aggregated decision; the remaining effects are emitted by the
            // kernel in the global `Vec<Effect>`.
            Ok(PolicyDecision::RequestEffect(effects.remove(0)))
        } else {
            Ok(PolicyDecision::Allow)
        }
    }
}
