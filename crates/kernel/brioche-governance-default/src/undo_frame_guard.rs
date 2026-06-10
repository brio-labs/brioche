//! UndoFrameGuard — `CycleRollbackPolicy` implementation (Book II §5.15).
//!
//! Provides a granular COW snapshot mechanism: only extensions that are
//! actually mutated are cloned via the VTable's `clone_box`, at the time of
//! the first write in the hook. Rollback restores only if the cumulative
//! weight stays under the `max_cow_bytes_per_hook` threshold.
//!
//! Refs: I-Gov-Rollback-BestEffort

use std::any::{Any, TypeId};
use std::collections::BTreeSet;

use brioche_core::{CycleRollbackPolicy, ExtVTable, ExtensionStorage, SnapshotStrategy};

/// COW frame guard with granular snapshot.
///
/// Each monitored hook starts a new empty frame. Upon the first mutation of
/// an extension via `get_mut`, the VTable `clone_box` is invoked to create a
/// backup copy. At the end of the hook, `commit_hook` discards the snapshots,
/// or `rollback_hook` restores them into `ExtensionStorage`.
///
/// # Configuration
///
/// The default threshold is 64 KB, covering >99% of extensions on the hot path
/// in reference profiles.
///
/// # Complexity
/// `on_mutation`: O(1) lookup + O(clone cost). `rollback_hook`: O(k) restores
/// where k = snapshotted types.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Gov-Rollback-BestEffort
pub struct UndoFrameGuard {
    max_cow_bytes_per_hook: usize,
    active_frame: Option<Vec<(TypeId, Box<dyn Any + Send + Sync>)>>,
    current_frame_weight: usize,
    snapshotted_types: BTreeSet<TypeId>,
}

impl UndoFrameGuard {
    /// Creates a guard with the default threshold of 64 KB.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self {
            max_cow_bytes_per_hook: 65536,
            active_frame: None,
            current_frame_weight: 0,
            snapshotted_types: BTreeSet::new(),
        }
    }

    /// Creates a guard with a custom threshold (in bytes).
    ///
    /// Refs: I-Gov-TraitAtomic
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

    fn commit_hook(&mut self, ext: &mut ExtensionStorage) {
        let log = ext.get_or_insert_default::<brioche_core::RollbackEventLog>();
        log.events.push(brioche_core::RollbackEvent {
            hook_name: String::new(),
            was_rollback: false,
            frame_weight: self.current_frame_weight,
            budget_exceeded: self.current_frame_weight >= self.max_cow_bytes_per_hook,
        });
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }

    fn rollback_hook(&mut self, ext: &mut ExtensionStorage) {
        let budget_exceeded = self.current_frame_weight >= self.max_cow_bytes_per_hook;
        if self.current_frame_weight < self.max_cow_bytes_per_hook
            && let Some(frame) = self.active_frame.take()
        {
            for (type_id, backup) in frame {
                ext.restore_boxed(type_id, backup);
            }
        }
        let log = ext.get_or_insert_default::<brioche_core::RollbackEventLog>();
        log.events.push(brioche_core::RollbackEvent {
            hook_name: String::new(),
            was_rollback: true,
            frame_weight: self.current_frame_weight,
            budget_exceeded,
        });
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }
}
