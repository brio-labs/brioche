//! TieredUndoFrameGuard — Book II §5.24.
//!
//! COW rollback with grading by criticality tier:
//! - `Critical`: always cloned, exempt from `max_cow_bytes_per_hook`.
//! - `Standard`: subject to the standard threshold.
//! - `BestEffort`: subject to a reduced threshold (e.g., 25% of max).
//!
//! Refs: I-Gov-Tiered-Rollback, I-Gov-Rollback-Critical

use brioche_core::{CycleRollbackPolicy, ExtVTable, ExtensionStorage, SnapshotStrategy};
use std::any::{Any, TypeId};
use std::collections::BTreeSet;

/// Garde de frame COW à trois niveaux de criticité.
///
/// Les types `#[brioche(critical_state)]` (stratégie `CriticalFullClone`)
/// sont toujours restaurés. Les types standard et best-effort sont
/// soumis à des seuils différenciés.
pub struct TieredUndoFrameGuard {
    max_standard_bytes: usize,
    max_best_effort_bytes: usize,
    active_frame: Option<Vec<(TypeId, Box<dyn Any + Send + Sync>)>>,
    current_standard_weight: usize,
    current_best_effort_weight: usize,
    snapshotted_types: BTreeSet<TypeId>,
}

impl TieredUndoFrameGuard {
    /// Crée un garde avec les seuils par défaut :
    /// - Standard : 64 KB
    /// - BestEffort : 16 KB (25%)
    pub fn new() -> Self {
        Self {
            max_standard_bytes: 65536,
            max_best_effort_bytes: 16384,
            active_frame: None,
            current_standard_weight: 0,
            current_best_effort_weight: 0,
            snapshotted_types: BTreeSet::new(),
        }
    }

    /// Crée un garde avec des seuils personnalisés.
    pub fn with_thresholds(max_standard_bytes: usize, max_best_effort_bytes: usize) -> Self {
        Self {
            max_standard_bytes,
            max_best_effort_bytes,
            active_frame: None,
            current_standard_weight: 0,
            current_best_effort_weight: 0,
            snapshotted_types: BTreeSet::new(),
        }
    }
}

impl Default for TieredUndoFrameGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl CycleRollbackPolicy for TieredUndoFrameGuard {
    fn begin_hook(&mut self) {
        self.active_frame = Some(Vec::new());
        self.current_standard_weight = 0;
        self.current_best_effort_weight = 0;
        self.snapshotted_types.clear();
    }

    fn on_mutation(&mut self, type_id: TypeId, vtable: &ExtVTable, current: &dyn Any) {
        if self.snapshotted_types.contains(&type_id) {
            return;
        }

        match vtable.snapshot_strategy {
            SnapshotStrategy::NoSnapshot => {
                self.snapshotted_types.insert(type_id);
            }
            SnapshotStrategy::CriticalFullClone => {
                let clone = (vtable.clone_box)(current);
                if let Some(frame) = &mut self.active_frame {
                    frame.push((type_id, clone));
                }
                self.snapshotted_types.insert(type_id);
            }
            SnapshotStrategy::FullClone => {
                let weight = (vtable.estimated_weight_bytes)(current);
                if self.current_standard_weight + weight > self.max_standard_bytes {
                    self.snapshotted_types.insert(type_id);
                    return;
                }
                let clone = (vtable.clone_box)(current);
                if let Some(frame) = &mut self.active_frame {
                    frame.push((type_id, clone));
                }
                self.current_standard_weight += weight;
                self.snapshotted_types.insert(type_id);
            }
            SnapshotStrategy::Incremental => {
                // Treat incremental as best-effort.
                let weight = (vtable.estimated_weight_bytes)(current);
                if self.current_best_effort_weight + weight > self.max_best_effort_bytes {
                    self.snapshotted_types.insert(type_id);
                    return;
                }
                let clone = (vtable.clone_box)(current);
                if let Some(frame) = &mut self.active_frame {
                    frame.push((type_id, clone));
                }
                self.current_best_effort_weight += weight;
                self.snapshotted_types.insert(type_id);
            }
        }
    }

    fn commit_hook(&mut self) {
        self.active_frame = None;
        self.current_standard_weight = 0;
        self.current_best_effort_weight = 0;
        self.snapshotted_types.clear();
    }

    fn rollback_hook(&mut self, ext: &mut ExtensionStorage) {
        let can_restore = self.current_standard_weight < self.max_standard_bytes
            && self.current_best_effort_weight < self.max_best_effort_bytes;

        if can_restore && let Some(frame) = self.active_frame.take() {
            for (type_id, backup) in frame {
                ext.restore_boxed(type_id, backup);
            }
        }
        self.active_frame = None;
        self.current_standard_weight = 0;
        self.current_best_effort_weight = 0;
        self.snapshotted_types.clear();
    }
}
