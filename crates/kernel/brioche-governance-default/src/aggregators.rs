//! Decision aggregation and effect constraints — Book II §5.
//!
//! Reference implementations:
//! - `LexicographicDecisionAggregator`: `DecisionAggregator`
//! - `FastHookEffectConstraint`: `HookEffectConstraint`
//!
//! Refs: I-Gov-Decision-Required, I-Core-HookEffect-O1

use brioche_core::{
    DecisionAggregator, Effect, EffectBit, ExtensionStorage, HistoryEdit, HookEffectConstraint,
    PluginResult, PolicyDecision,
};

// ---------------------------------------------------------------------------
// LexicographicDecisionAggregator
// ---------------------------------------------------------------------------

/// Deterministic lexicographic aggregator.
///
/// This component is **mandatory**: the kernel refuses to start without
/// a `DecisionAggregator` injected via `BriocheEngineBuilder`.
///
/// Refs: I-Gov-Decision-Required, I-Gov-Decision-Isolation
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
                    return Ok(PolicyDecision::Block { reason });
                }
                PolicyDecision::MutateHistory(mut e) => {
                    edits.append(&mut e);
                }
                PolicyDecision::RequestEffect(eff) => {
                    effects.push(eff);
                }
                PolicyDecision::OverrideTransition(ov) => {
                    return Ok(PolicyDecision::OverrideTransition(ov));
                }
                _ => {}
            }
        }

        if !edits.is_empty() {
            Ok(PolicyDecision::MutateHistory(edits))
        } else if !effects.is_empty() {
            Ok(PolicyDecision::RequestEffect(effects.remove(0)))
        } else {
            Ok(PolicyDecision::Allow)
        }
    }
}

// ---------------------------------------------------------------------------
// FastHookEffectConstraint
// ---------------------------------------------------------------------------

/// Effect constraint by pre-computed binary mask.
///
/// Each hook index (0–7) has a `u64` mask where each bit
/// represents an allowed `Effect` variant.
///
/// Refs: I-Core-HookEffect-O1
pub struct FastHookEffectConstraint {
    masks: [u64; 8],
}

impl FastHookEffectConstraint {
    /// Creates a constraint with the provided masks.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new(masks: [u64; 8]) -> Self {
        Self { masks }
    }

    /// Permissive profile: all effects allowed on all hooks.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn permissive() -> Self {
        Self {
            masks: [u64::MAX; 8],
        }
    }

    /// Standard profile: restricts dangerous effects on sensitive hooks.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn standard() -> Self {
        let mut masks = [0u64; 8];

        masks[0] = u64::MAX;

        masks[1] = EffectBit::FORWARD_TO_UI
            | EffectBit::ERROR
            | EffectBit::SAVE_SESSION
            | EffectBit::SYSTEM_IDLE;

        masks[2] = EffectBit::FORWARD_TO_UI | EffectBit::SAVE_SESSION | EffectBit::CALL_LLM_NETWORK;

        masks[3] = EffectBit::EXECUTE_CPU_TASK | EffectBit::SAVE_SESSION | EffectBit::FORWARD_TO_UI;

        masks[4] = EffectBit::SAVE_SESSION | EffectBit::FORWARD_TO_UI;

        masks[5] = EffectBit::EXECUTE_TOOLS | EffectBit::SAVE_SESSION | EffectBit::ERROR;

        masks[6] = EffectBit::CALL_LLM_NETWORK | EffectBit::SAVE_SESSION | EffectBit::FORWARD_TO_UI;

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
        false
    }
}
