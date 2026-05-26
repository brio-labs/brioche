//! StateConsistencyGuard — implémentation `ConsistencyVerifier` (Book II §5.2).
//!
//! Vérifie la cohérence mécanique après une transition. Si l'état est
//! `Predicting` ou `ExecutingTools` sans justification (pile vide),
//! force un retour à `Idle` avec nettoyage.
//!
//! Refs: I-Core-NoPanic, I-Gov-Decision-Required

use brioche_core::{
    AgentState, AgentStateTag, ConsistencyVerifier, Effect, ErrorCode, PluginResult, Session,
};

/// Vérificateur de cohérence mécanique d'état.
///
/// Ce garde est optionnel mais recommandé en production. Sans injection,
/// le kernel ne vérifie pas la cohérence — ce qui peut laisser l'automate
/// dans un état incohérent après un `OverrideTransition` mal formé.
pub struct StateConsistencyGuard;

impl StateConsistencyGuard {
    /// Crée une nouvelle instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for StateConsistencyGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsistencyVerifier for StateConsistencyGuard {
    fn verify_consistency(&self, session: &mut Session) -> PluginResult<Option<Vec<Effect>>> {
        let tag = AgentStateTag::from(&session.state);

        match tag {
            AgentStateTag::Predicting | AgentStateTag::ExecutingTools => {
                // Un état actif sans pile de contexte est incohérent :
                // il n'y a pas d'état précédent à restaurer.
                if session.state_stack.is_empty() {
                    let effects = vec![
                        Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            message: "inconsistent state: active without stack context".into(),
                        },
                        Effect::SaveSession,
                        Effect::SystemIdle,
                    ];

                    // Forçage mécanique vers Idle avec nettoyage.
                    session.state = AgentState::Idle;
                    session.state_stack.clear();
                    session.active_tools.clear();

                    return Ok(Some(effects));
                }

                // ExecutingTools sans outils actifs est également suspect,
                // mais le kernel gère déjà ce cas via `active_tools`.
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
