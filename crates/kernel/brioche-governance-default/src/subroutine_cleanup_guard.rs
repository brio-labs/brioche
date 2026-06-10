//! SubRoutineCleanupGuard — `SubRoutineLifecycleGuard` implementation (Book II §5.16).
//!
//! Cleans up the `SessionRegistry` on every outgoing transition from
//! the `SubRoutine` state, preventing the accumulation of orphaned sessions.
//!
//! Refs: I-Gov-SubRoutineLifecycle-Guard

use brioche_core::{
    Effect, PluginResult, Session, SessionRegistry, SubRoutineHandle, SubRoutineLifecycleGuard,
};

/// Sub-routine cleanup guard.
///
/// At instantiation, use `SubRoutineCleanupGuard::new()` to obtain
/// the default SDK reference.
///
/// # Algorithm
/// 1. Increments `registry.exit_counts[handle]` (defense in depth).
/// 2. Removes the child session from `SessionRegistry`.
/// 3. Emits `Effect::SaveSession` if the removal succeeds.
///
/// Refs: I-Gov-SubRoutineLifecycle-Guard
pub struct SubRoutineCleanupGuard;

impl SubRoutineCleanupGuard {
    /// Creates a new instance of the cleanup guard.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for SubRoutineCleanupGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl SubRoutineLifecycleGuard for SubRoutineCleanupGuard {
    fn on_exit(
        &self,
        handle: SubRoutineHandle,
        _parent: &mut Session,
        registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        registry.increment_exit_count(&handle);

        if registry.remove(&handle).is_some() {
            Ok(vec![Effect::SaveSession])
        } else {
            // Already cleaned up — no additional effect.
            Ok(vec![])
        }
    }
}
