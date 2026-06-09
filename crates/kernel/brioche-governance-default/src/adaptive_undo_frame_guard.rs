//! AdaptiveUndoFrameGuard — Book II §5.20.
//!
//! Adaptive COW rollback policy that consults `CowBudgetPolicy` before
//! deciding whether to snapshot a mutation. Falls back to the static
//! threshold if no `CowBudgetPolicy` is injected.
//!
//! Refs: I-Gov-CowBudget-Adaptative, I-Gov-Rollback-BestEffort

use std::any::{Any, TypeId};
use std::collections::BTreeSet;

use brioche_core::{
    CowBudgetPolicy, CycleRollbackPolicy, ExtVTable, ExtensionStorage, SnapshotStrategy,
};

/// Adaptive COW frame guard.
///
/// Like `UndoFrameGuard`, but the threshold is determined dynamically
/// by consulting a `CowBudgetPolicy` if available.
pub struct AdaptiveUndoFrameGuard {
    fallback_max_cow_bytes: usize,
    budget_policy: Option<Box<dyn CowBudgetPolicy>>,
    active_frame: Option<Vec<(TypeId, Box<dyn Any + Send + Sync>)>>,
    current_frame_weight: usize,
    snapshotted_types: BTreeSet<TypeId>,
    current_hook: String,
}

impl AdaptiveUndoFrameGuard {
    /// Creates a guard with the default fallback threshold of 64 KB.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self {
            fallback_max_cow_bytes: 65536,
            budget_policy: None,
            active_frame: None,
            current_frame_weight: 0,
            snapshotted_types: BTreeSet::new(),
            current_hook: String::new(),
        }
    }

    /// Creates a guard with a custom fallback threshold.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_fallback_max(fallback_max_cow_bytes: usize) -> Self {
        Self {
            fallback_max_cow_bytes,
            budget_policy: None,
            active_frame: None,
            current_frame_weight: 0,
            snapshotted_types: BTreeSet::new(),
            current_hook: String::new(),
        }
    }

    /// Attaches a dynamic `CowBudgetPolicy` for per-hook budget queries.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_budget_policy(mut self, policy: Box<dyn CowBudgetPolicy>) -> Self {
        self.budget_policy = Some(policy);
        self
    }

    fn effective_max(&self) -> usize {
        match &self.budget_policy {
            Some(policy) => policy.max_cow_bytes(&self.current_hook),
            None => self.fallback_max_cow_bytes,
        }
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
        let max = self.effective_max();

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

    fn commit_hook(&mut self, ext: &mut ExtensionStorage) {
        let log = ext.get_or_insert_default::<brioche_core::RollbackEventLog>();
        log.events.push(brioche_core::RollbackEvent {
            hook_name: self.current_hook.clone(),
            was_rollback: false,
            frame_weight: self.current_frame_weight,
            budget_exceeded: self.current_frame_weight >= self.effective_max(),
        });
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }

    fn rollback_hook(&mut self, ext: &mut ExtensionStorage) {
        let max = self.effective_max();
        let budget_exceeded = self.current_frame_weight >= max;
        if self.current_frame_weight < max
            && let Some(frame) = self.active_frame.take()
        {
            for (type_id, backup) in frame {
                ext.restore_boxed(type_id, backup);
            }
        }
        let log = ext.get_or_insert_default::<brioche_core::RollbackEventLog>();
        log.events.push(brioche_core::RollbackEvent {
            hook_name: self.current_hook.clone(),
            was_rollback: true,
            frame_weight: self.current_frame_weight,
            budget_exceeded,
        });
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }
}
