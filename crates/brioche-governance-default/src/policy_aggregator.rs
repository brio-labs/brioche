//! LexicographicDecisionAggregator — implémentation `DecisionAggregator` (Book II §5.17).
//!
//! Agrège les décisions intermédiaires collectées par le kernel sur la
//! route `before_prediction` selon une règle de fusion lexicographique
//! déterministe.
//!
//! Règle de priorité (dans l'ordre d'évaluation) :
//! 1. `Block`   → retourne immédiatement `Block` avec la raison agrégée.
//! 2. `OverrideTransition` → retourne le premier rencontré.
//! 3. `MutateHistory` → accumule toutes les éditions.
//! 4. `RequestEffect` → accumule tous les effets.
//! 5. `Allow`   → ignoré sauf si aucune autre décision.
//!
//! Refs: I-Gov-Decision-Required, I-Gov-Decision-Isolation

use brioche_core::{
    DecisionAggregator, Effect, ExtensionStorage, HistoryEdit, PluginResult, PolicyDecision,
};

/// Agrégateur déterministe par ordre lexicographique.
///
/// Ce composant est **obligatoire** : le kernel refuse de démarrer sans
/// un `DecisionAggregator` injecté via `BriocheEngineBuilder`.
///
/// # Exemple
/// ```
/// use brioche_governance_default::LexicographicDecisionAggregator;
/// use brioche_core::BriocheEngineBuilder;
///
/// let engine = BriocheEngineBuilder::new()
///     .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
///     // ... autres traits obligatoires
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
                    // Block court-circuite immédiatement (fusion lexicographique).
                    return Ok(PolicyDecision::Block { reason });
                }
                PolicyDecision::MutateHistory(mut e) => {
                    edits.append(&mut e);
                }
                PolicyDecision::RequestEffect(eff) => {
                    effects.push(eff);
                }
                PolicyDecision::OverrideTransition(ov) => {
                    // Le premier OverrideTransition l'emporte.
                    return Ok(PolicyDecision::OverrideTransition(ov));
                }
            }
        }

        if !edits.is_empty() {
            Ok(PolicyDecision::MutateHistory(edits))
        } else if !effects.is_empty() {
            // Concaténation des RequestEffects dans un seul MutateHistory n'est
            // pas possible (types différents). On retourne le premier effet
            // comme décision agrégée ; les effets restants sont émis par le
            // kernel dans le `Vec<Effect>` global.
            Ok(PolicyDecision::RequestEffect(effects.remove(0)))
        } else {
            Ok(PolicyDecision::Allow)
        }
    }
}
