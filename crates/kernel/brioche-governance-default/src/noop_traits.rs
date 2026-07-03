//! Noop* reference implementations — Book II §5.
//!
//! Null implementations of optional governance traits. These are used
//! by the `Permissive` profile and as placeholders when a trait is not
//! required.
//!
//! Refs: I-Gov-Profile-Agnostic

use std::any::{Any, TypeId};

use brioche_core::{
    CowBudgetPolicy, CycleRollbackPolicy, Effect, ExtensionStorage, GovernanceFailoverHandler,
    HookEffectConstraint, PluginResult, Session,
};

// ---------------------------------------------------------------------------
// NoopEpochInterceptor — not needed because EpochGuard is always injected.
// NoopSubRoutineHandler — not needed because SubRoutineOrchestrator is always injected.
// NoopConsistencyVerifier — not needed because StateConsistencyGuard is always injected.
// ---------------------------------------------------------------------------

/// Null `GovernanceFailoverHandler`.
///
/// Passes through plugin faults without intervention.
///
/// Refs: I-Gov-TraitAtomic
pub struct NoopGovernanceFailoverHandler;

impl GovernanceFailoverHandler for NoopGovernanceFailoverHandler {
    fn handle_failure(
        &self,
        _session: &mut Session,
        _fault: &Effect,
    ) -> PluginResult<Option<Vec<Effect>>> {
        Ok(None)
    }
}

/// Null `HookEffectConstraint`.
///
/// Allows all effects on all hooks (same as not injecting the trait).
///
/// Refs: I-Gov-TraitAtomic
/// Refs: I-Gov-TraitAtomic, I-Core-HookEffect-O1
pub struct NoopHookEffectConstraint;

impl HookEffectConstraint for NoopHookEffectConstraint {
    /// O(1). Always returns `true`.
    fn is_allowed_fast(&self, _hook_index: u8, _effect_mask: u64) -> bool {
        true
    }

    fn is_allowed_fallback(&self, _hook_name: &str, _effect_variant: &str) -> bool {
        true
    }
}

/// Null `CowBudgetPolicy`.
///
/// Returns the default 64 KB threshold for all hooks.
/// Refs: I-Gov-TraitAtomic
///
/// Refs: I-Gov-Rollback-BestEffort
pub struct NoopCowBudgetPolicy;

impl CowBudgetPolicy for NoopCowBudgetPolicy {
    fn max_cow_bytes(&self, _hook_name: &str) -> usize {
        65536
    }
}

/// Null `CycleRollbackPolicy`.
///
/// All methods are no-ops. This is the behavior of the kernel when
/// Refs: I-Gov-TraitAtomic
/// no `CycleRollbackPolicy` is injected.
///
/// Refs: I-Gov-Rollback-BestEffort
pub struct NoopCycleRollbackPolicy;

impl CycleRollbackPolicy for NoopCycleRollbackPolicy {
    fn begin_hook(&mut self, _hook_name: &'static str) {}

    fn on_mutation(
        &mut self,
        _type_id: TypeId,
        _vtable: &brioche_core::ExtVTable,
        _current: &dyn Any,
    ) {
    }

    fn commit_hook(&mut self, _ext: &mut ExtensionStorage) {}

    fn rollback_hook(&mut self, _ext: &mut ExtensionStorage) {}
}

/// Permissive `HookEffectConstraint`.
///
/// **Dev / prototyping only.** Allows all standard and future effects on
/// all hooks. This disables the effect-safety layer and should never be
/// used in production.
///
/// Refs: I-Gov-TraitAtomic, I-Core-HookEffect-O1
pub struct PermissiveHookEffectConstraint {
    masks: [u64; 8],
}

impl PermissiveHookEffectConstraint {
    /// Creates a fully permissive constraint (all effects allowed).
    ///
    /// # Warning
    /// This disables all effect restrictions. It is intended for local
    /// development and prototyping only.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        tracing::warn!(
            "PermissiveHookEffectConstraint constructed: all effects are allowed. \
             Use only for development / prototyping."
        );
        Self {
            masks: [u64::MAX; 8],
        }
    }
}

impl Default for PermissiveHookEffectConstraint {
    fn default() -> Self {
        Self::new()
    }
}

impl HookEffectConstraint for PermissiveHookEffectConstraint {
    /// O(1). Pre-computed binary mask.
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool {
        if hook_index >= 8 {
            return true;
        }
        (self.masks[hook_index as usize] & effect_mask) != 0
    }

    fn is_allowed_fallback(&self, _hook_name: &str, _effect_variant: &str) -> bool {
        true
    }
}
