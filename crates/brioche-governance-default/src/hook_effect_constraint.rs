//! FastHookEffectConstraint — implémentation `HookEffectConstraint` (Book II §2.7).
//!
//! Validation O(1) des effets autorisés par hook via masque binaire.
//!
//! Refs: I-Core-HookEffect-O1

use brioche_core::{EffectBit, HookEffectConstraint};

/// Contrainte d'effet par masque binaire pré-calculé.
///
/// Chaque index de hook (0–7) possède un masque `u64` où chaque bit
/// représente un variant `Effect` autorisé. La validation est une simple
/// opération bitwise : `(mask & effect_mask) != 0`.
///
/// # Hook indices
/// | Index | Hook |
/// |-------|------|
/// | 0 | `transition` (fallback global) |
/// | 1 | `on_input` |
/// | 2 | `before_prediction` |
/// | 3 | `on_stream_event` |
/// | 4 | `after_prediction` |
/// | 5 | `on_tool_calls` |
/// | 6 | `on_tool_result` |
/// | 7 | `on_error` |
///
/// # Exemple
/// ```
/// use brioche_governance_default::FastHookEffectConstraint;
/// use brioche_core::HookEffectConstraint;
///
/// let constraint = FastHookEffectConstraint::permissive();
/// assert!(constraint.is_allowed_fast(1, brioche_core::EffectBit::FORWARD_TO_UI));
/// ```
pub struct FastHookEffectConstraint {
    masks: [u64; 8],
}

impl FastHookEffectConstraint {
    /// Crée une contrainte avec les masques fournis.
    pub fn new(masks: [u64; 8]) -> Self {
        Self { masks }
    }

    /// Profil permissif : tous les effets autorisés sur tous les hooks.
    pub fn permissive() -> Self {
        Self {
            masks: [u64::MAX; 8],
        }
    }

    /// Profil standard : restreint les effets dangereux sur les hooks sensibles.
    ///
    /// - `on_input` : `ForwardToUi`, `Error`, `SaveSession`, `SystemIdle`
    /// - `before_prediction` : `MutateHistory` (via RequestEffect),
    ///   `ForwardToUi`, `SaveSession`
    /// - `on_stream_event` : `ExecuteCpuTask`, `SaveSession`
    /// - `on_tool_calls` : `ExecuteTools` (via RequestEffect), `SaveSession`
    /// - `on_tool_result` : `CallLlmNetwork`, `SaveSession`
    pub fn standard() -> Self {
        let mut masks = [0u64; 8];

        // Index 0 : fallback global (transition)
        masks[0] = u64::MAX;

        // on_input
        masks[1] = EffectBit::FORWARD_TO_UI
            | EffectBit::ERROR
            | EffectBit::SAVE_SESSION
            | EffectBit::SYSTEM_IDLE;

        // before_prediction
        masks[2] = EffectBit::FORWARD_TO_UI | EffectBit::SAVE_SESSION | EffectBit::CALL_LLM_NETWORK;

        // on_stream_event
        masks[3] = EffectBit::EXECUTE_CPU_TASK | EffectBit::SAVE_SESSION | EffectBit::FORWARD_TO_UI;

        // after_prediction
        masks[4] = EffectBit::SAVE_SESSION | EffectBit::FORWARD_TO_UI;

        // on_tool_calls
        masks[5] = EffectBit::EXECUTE_TOOLS | EffectBit::SAVE_SESSION | EffectBit::ERROR;

        // on_tool_result
        masks[6] = EffectBit::CALL_LLM_NETWORK | EffectBit::SAVE_SESSION | EffectBit::FORWARD_TO_UI;

        // on_error
        masks[7] = EffectBit::FORWARD_TO_UI
            | EffectBit::SAVE_SESSION
            | EffectBit::SYSTEM_IDLE
            | EffectBit::REBUILD_ROUTES;

        Self { masks }
    }
}

impl HookEffectConstraint for FastHookEffectConstraint {
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool {
        if hook_index >= 8 {
            return false;
        }
        (self.masks[hook_index as usize] & effect_mask) != 0
    }

    fn is_allowed_fallback(&self, _hook_name: &str, _effect_variant: &str) -> bool {
        // Par défaut strict : aucun effet non standard autorisé.
        false
    }
}
