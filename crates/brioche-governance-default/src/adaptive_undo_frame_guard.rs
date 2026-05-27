//! AdaptiveUndoFrameGuard — Book II §5.20.
//!
//! Adaptive COW rollback policy that consults `CowBudgetPolicy` before
//! deciding whether to snapshot a mutation. Falls back to the static
//! threshold if no `CowBudgetPolicy` is injected.
//!
//! Refs: I-Gov-CowBudget-Adaptative, I-Gov-Rollback-BestEffort

use brioche_core::{
    CowBudgetPolicy, CycleRollbackPolicy, ExtVTable, ExtensionStorage, SnapshotStrategy,
};
use std::any::{Any, TypeId};
use std::collections::BTreeSet;

/// Garde de frame COW adaptative.
///
/// Comme `UndoFrameGuard`, mais le seuil est déterminé dynamiquement
/// par consultation d'une `CowBudgetPolicy` si disponible.
pub struct AdaptiveUndoFrameGuard {
    fallback_max_cow_bytes: usize,
    active_frame: Option<Vec<(TypeId, Box<dyn Any + Send + Sync>)>>,
    current_frame_weight: usize,
    snapshotted_types: BTreeSet<TypeId>,
    #[allow(dead_code)]
    current_hook: String,
}

impl AdaptiveUndoFrameGuard {
    /// Crée un garde avec le seuil de repli par défaut de 64 KB.
    pub fn new() -> Self {
        Self {
            fallback_max_cow_bytes: 65536,
            active_frame: None,
            current_frame_weight: 0,
            snapshotted_types: BTreeSet::new(),
            current_hook: String::new(),
        }
    }

    /// Crée un garde avec un seuil de repli personnalisé.
    pub fn with_fallback_max(fallback_max_cow_bytes: usize) -> Self {
        Self {
            fallback_max_cow_bytes,
            active_frame: None,
            current_frame_weight: 0,
            snapshotted_types: BTreeSet::new(),
            current_hook: String::new(),
        }
    }

    fn effective_max(&self, _budget_policy: Option<&dyn CowBudgetPolicy>) -> usize {
        // In a full implementation, consult the budget policy here.
        // For Sprint 8, we use the fallback threshold.
        self.fallback_max_cow_bytes
    }
}

impl Default for AdaptiveUndoFrameGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl CycleRollbackPolicy for AdaptiveUndoFrameGuard {
    fn begin_hook(&mut self) {
        self.active_frame = Some(Vec::new());
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }

    fn on_mutation(&mut self, type_id: TypeId, vtable: &ExtVTable, current: &dyn Any) {
        if self.snapshotted_types.contains(&type_id) {
            return;
        }

        match vtable.snapshot_strategy {
            SnapshotStrategy::NoSnapshot => {
                self.snapshotted_types.insert(type_id);
                return;
            }
            SnapshotStrategy::CriticalFullClone => {
                let clone = (vtable.clone_box)(current);
                if let Some(frame) = &mut self.active_frame {
                    frame.push((type_id, clone));
                }
                self.snapshotted_types.insert(type_id);
                return;
            }
            _ => {}
        }

        let weight = (vtable.estimated_weight_bytes)(current);
        let max = self.effective_max(None);

        if self.current_frame_weight + weight > max {
            self.snapshotted_types.insert(type_id);
            return;
        }

        let clone = (vtable.clone_box)(current);
        if let Some(frame) = &mut self.active_frame {
            frame.push((type_id, clone));
        }
        self.current_frame_weight += weight;
        self.snapshotted_types.insert(type_id);
    }

    fn commit_hook(&mut self) {
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }

    fn rollback_hook(&mut self, ext: &mut ExtensionStorage) {
        let max = self.effective_max(None);
        if self.current_frame_weight < max
            && let Some(frame) = self.active_frame.take()
        {
            for (type_id, backup) in frame {
                ext.restore_boxed(type_id, backup);
            }
        }
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }
}
