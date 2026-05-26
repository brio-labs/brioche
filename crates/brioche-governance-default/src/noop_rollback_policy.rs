//! NoopCycleRollbackPolicy — implémentation nulle de `CycleRollbackPolicy` (Book II §5.18).
//!
//! Fournie comme implémentation par défaut quand le rollback COW n'est pas
//! requis. Le kernel émet toujours `PluginFault` en cas de dépassement de
//! budget, mais aucune restauration d'état n'est effectuée.
//!
//! Refs: I-Gov-Rollback-BestEffort

use brioche_core::{CycleRollbackPolicy, ExtensionStorage};
use std::any::{Any, TypeId};

/// Rollback policy nulle.
///
/// Toutes les méthodes sont des no-ops. C'est le comportement par défaut
/// du kernel lorsqu'aucune `CycleRollbackPolicy` n'est injectée.
pub struct NoopCycleRollbackPolicy;

impl CycleRollbackPolicy for NoopCycleRollbackPolicy {
    fn begin_hook(&mut self) {}

    fn on_mutation(
        &mut self,
        _type_id: TypeId,
        _vtable: &brioche_core::ExtVTable,
        _current: &dyn Any,
    ) {
    }

    fn commit_hook(&mut self) {}

    fn rollback_hook(&mut self, _ext: &mut ExtensionStorage) {}
}
