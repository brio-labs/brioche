//! EpochGuard — `EpochInterceptor` reference (Book II §5.1).
//!
//! Intercepts inputs carrying an obsolete `generation_id`.
//! Epoch increment on `SystemSignal` is handled by the shell
//! via the dedicated adapter (Sprint 8+).
//!
//! Refs: I-Gov-Epoch-Reject, I-Comp-Epoch-First

use brioche_core::{
    EngineInput, EpochAction, EpochInterceptor, EpochState, ExtensionStorage, PluginResult,
};

/// Temporal barrier manager by epochs.
///
/// `intercept_epoch` compares the `generation_id` carried by an
/// `EngineInput::ToolCallsResult` with `EpochState.current_generation`.
/// In case of divergence, the input is silently rejected.
///
/// # Invariants
/// - Refs: I-Gov-Epoch-Reject — rejects asynchronous responses from past epochs.
/// - Refs: I-Comp-Epoch-First — always evaluated first in the cycle.
pub struct EpochGuard;

impl EpochInterceptor for EpochGuard {
    fn intercept_epoch(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<EpochAction> {
        let epoch_state = ext.get_or_insert_default::<EpochState>();

        match input {
            // Only ToolCallsResults carry a verifiable generation_id
            // at this stage. Other inputs (UserMessage, LlmStream) are
            // always treated as belonging to the current epoch.
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
