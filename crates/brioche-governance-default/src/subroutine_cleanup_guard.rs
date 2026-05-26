//! SubRoutineCleanupGuard — implémentation `SubRoutineLifecycleGuard` (Book II §5.16).
//!
//! Nettoie le `SessionRegistry` à chaque transition sortante depuis
//! l'état `SubRoutine`, empêche l'accumulation de sessions orphelines.
//!
//! Refs: I-Gov-SubRoutineLifecycle-Guard

use brioche_core::{
    Effect, PluginResult, Session, SessionRegistry, SubRoutineHandle, SubRoutineLifecycleGuard,
};

/// Garde de nettoyage des sous-routines.
///
/// À l'instanciation, utilisez `SubRoutineCleanupGuard::new()` pour obtenir
/// la référence par défaut du SDK.
///
/// # Algorithme
/// 1. Incrémente `registry.exit_counts[handle]` (défense en profondeur).
/// 2. Retire la session enfant de `SessionRegistry`.
/// 3. Émet `Effect::SaveSession` si la suppression réussit.
pub struct SubRoutineCleanupGuard;

impl SubRoutineCleanupGuard {
    /// Crée une nouvelle instance du garde de nettoyage.
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
            // Déjà nettoyé — pas d'effet supplémentaire.
            Ok(vec![])
        }
    }
}
