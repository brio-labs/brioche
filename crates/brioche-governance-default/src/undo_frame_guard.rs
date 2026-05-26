//! UndoFrameGuard — implémentation `CycleRollbackPolicy` (Book II §5.15).
//!
//! Fournit un mécanisme de snapshot COW granulaire : seules les extensions
//! réellement mutées sont clonées via `clone_box` du VTable, au moment de la
//! première écriture dans le hook. Le rollback restaure uniquement si le poids
//! cumulé reste sous le seuil `max_cow_bytes_per_hook`.
//!
//! Refs: I-Gov-Rollback-BestEffort

use brioche_core::{CycleRollbackPolicy, ExtVTable, ExtensionStorage, SnapshotStrategy};
use std::any::{Any, TypeId};
use std::collections::BTreeSet;

/// Garde de frame COW avec snapshot granulaire.
///
/// Chaque hook monitoré démarre une nouvelle frame vide. Lors de la première
/// mutation d'une extension via `get_mut`, le VTable `clone_box` est invoqué
/// pour créer une copie de sauvegarde. En fin de hook, `commit_hook` discard
/// les snapshots, ou `rollback_hook` les restaure dans `ExtensionStorage`.
///
/// # Configuration
///
/// Le seuil par défaut est 64 KB, couvrant >99 % des extensions du hot path
/// dans les profils de référence.
pub struct UndoFrameGuard {
    max_cow_bytes_per_hook: usize,
    active_frame: Option<Vec<(TypeId, Box<dyn Any + Send + Sync>)>>,
    current_frame_weight: usize,
    snapshotted_types: BTreeSet<TypeId>,
}

impl UndoFrameGuard {
    /// Crée un garde avec le seuil par défaut de 64 KB.
    pub fn new() -> Self {
        Self {
            max_cow_bytes_per_hook: 65536,
            active_frame: None,
            current_frame_weight: 0,
            snapshotted_types: BTreeSet::new(),
        }
    }

    /// Crée un garde avec un seuil personnalisé (en octets).
    pub fn with_max_cow_bytes(max_cow_bytes_per_hook: usize) -> Self {
        Self {
            max_cow_bytes_per_hook,
            active_frame: None,
            current_frame_weight: 0,
            snapshotted_types: BTreeSet::new(),
        }
    }
}

impl Default for UndoFrameGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl CycleRollbackPolicy for UndoFrameGuard {
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
                // Always snapshot critical types, ignoring threshold.
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

        if self.current_frame_weight + weight > self.max_cow_bytes_per_hook {
            // Abandon snapshot for this mutation — best-effort rollback.
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
        if self.current_frame_weight < self.max_cow_bytes_per_hook
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
