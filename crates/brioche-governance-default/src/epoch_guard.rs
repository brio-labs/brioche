//! EpochGuard — référence `EpochInterceptor` (Book II §5.1).
//!
//! Intercepte les inputs portant un `generation_id` obsolète.
//! L'incrémentation d'époch sur `SystemSignal` est gérée par le shell
//! via l'adaptateur dédié (Sprint 8+).
//!
//! Refs: I-Gov-Epoch-Reject, I-Comp-Epoch-First

use brioche_core::{
    EngineInput, EpochAction, EpochInterceptor, EpochState, ExtensionStorage, PluginResult,
};

/// Gestionnaire de barrière temporelle par époques.
///
/// `intercept_epoch` compare le `generation_id` porté par un
/// `EngineInput::ToolCallsResult` avec `EpochState.current_generation`.
/// En cas de divergence, l'input est silencieusement rejeté.
///
/// # Invariants
/// - Refs: I-Gov-Epoch-Reject — rejette les réponses asynchrones d'époques passées.
/// - Refs: I-Comp-Epoch-First — toujours évalué en premier dans le cycle.
pub struct EpochGuard;

impl EpochInterceptor for EpochGuard {
    fn intercept_epoch(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<EpochAction> {
        let epoch_state = ext.get_or_insert_default::<EpochState>();

        match input {
            // Seuls les ToolCallsResult portent un generation_id vérifiable
            // à ce stade. Les autres inputs (UserMessage, LlmStream) sont
            // toujours traités comme appartenant à l'époque courante.
            EngineInput::ToolCallsResult { generation_id, .. } => {
                if *generation_id != epoch_state.current_generation {
                    return Ok(EpochAction::Block {
                        reason: format!(
                            "epoch mismatch: expected {}, got {}",
                            epoch_state.current_generation, generation_id
                        ),
                    });
                }
                Ok(EpochAction::Proceed)
            }
            _ => Ok(EpochAction::Proceed),
        }
    }
}
