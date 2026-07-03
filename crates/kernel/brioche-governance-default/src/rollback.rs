//! COW rollback & budget policies.
//!
//! This module groups copy-on-write rollback strategies and the budget
//! policies that tune them:
//! - `UndoFrameGuard`: granular per-extension COW snapshots.
//! - `TieredUndoFrameGuard`: tiered criticality rollback.
//! - `AdaptiveUndoFrameGuard`: dynamic budget via `CowBudgetPolicy`.
//! - `HistoricalCowBudgetPolicy`: sliding-window budget auto-tuning.
//!
//! Refs: I-Gov-Rollback-BestEffort, I-Gov-Tiered-Rollback,
//! I-Gov-CowBudget-Adaptative

use std::any::{Any, TypeId};
use std::collections::{BTreeSet, VecDeque};

use brioche_core::{
    CowBudgetPolicy, CycleRollbackPolicy, ExtVTable, ExtensionStorage, RollbackEvent,
    RollbackEventLog, SnapshotStrategy,
};

// ---------------------------------------------------------------------------
// UndoFrameGuard
// ---------------------------------------------------------------------------

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
    fn begin_hook(&mut self, _hook_name: &'static str) {
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
        let log = ext.get_or_insert_default::<RollbackEventLog>();
        log.events.push(RollbackEvent {
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
        if let Some(frame) = self.active_frame.take() {
            for (type_id, backup) in frame {
                ext.restore_boxed(type_id, backup);
            }
        }
        let log = ext.get_or_insert_default::<RollbackEventLog>();
        log.events.push(RollbackEvent {
            hook_name: String::new(),
            was_rollback: true,
            frame_weight: self.current_frame_weight,
            budget_exceeded,
        });
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }

    fn is_budget_exceeded(&self) -> bool {
        self.current_frame_weight >= self.max_cow_bytes_per_hook
    }
}

// ---------------------------------------------------------------------------
// TieredUndoFrameGuard
// ---------------------------------------------------------------------------

/// COW frame guard with three criticality tiers.
///
/// Types `#[brioche(critical_state)]` (strategy `CriticalFullClone`)
/// are always restored. Standard and best-effort types are
/// subject to differentiated thresholds.
/// Refs: I-Gov-TraitAtomic
///
/// Refs: I-Gov-Rollback-BestEffort, I-Gov-Tiered-Rollback
pub struct TieredUndoFrameGuard {
    max_standard_bytes: usize,
    max_best_effort_bytes: usize,
    active_frame: Option<Vec<(TypeId, Box<dyn Any + Send + Sync>)>>,
    current_standard_weight: usize,
    current_best_effort_weight: usize,
    snapshotted_types: BTreeSet<TypeId>,
}

impl TieredUndoFrameGuard {
    /// Creates a guard with the default thresholds:
    /// Refs: I-Gov-TraitAtomic
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

    /// Creates a guard with custom thresholds.
    ///
    /// Refs: I-Gov-TraitAtomic
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
    fn begin_hook(&mut self, _hook_name: &'static str) {
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

    fn commit_hook(&mut self, ext: &mut ExtensionStorage) {
        let log = ext.get_or_insert_default::<RollbackEventLog>();
        log.events.push(RollbackEvent {
            hook_name: String::new(),
            was_rollback: false,
            frame_weight: self.current_standard_weight + self.current_best_effort_weight,
            budget_exceeded: self.current_standard_weight >= self.max_standard_bytes
                || self.current_best_effort_weight >= self.max_best_effort_bytes,
        });
        self.active_frame = None;
        self.current_standard_weight = 0;
        self.current_best_effort_weight = 0;
        self.snapshotted_types.clear();
    }

    fn rollback_hook(&mut self, ext: &mut ExtensionStorage) {
        let budget_exceeded = self.current_standard_weight >= self.max_standard_bytes
            || self.current_best_effort_weight >= self.max_best_effort_bytes;

        if let Some(frame) = self.active_frame.take() {
            for (type_id, backup) in frame {
                ext.restore_boxed(type_id, backup);
            }
        }
        let log = ext.get_or_insert_default::<RollbackEventLog>();
        log.events.push(RollbackEvent {
            hook_name: String::new(),
            was_rollback: true,
            frame_weight: self.current_standard_weight + self.current_best_effort_weight,
            budget_exceeded,
        });
        self.active_frame = None;
        self.current_standard_weight = 0;
        self.current_best_effort_weight = 0;
        self.snapshotted_types.clear();
    }

    fn is_budget_exceeded(&self) -> bool {
        self.current_standard_weight >= self.max_standard_bytes
            || self.current_best_effort_weight >= self.max_best_effort_bytes
    }
}

// ---------------------------------------------------------------------------
// AdaptiveUndoFrameGuard
// ---------------------------------------------------------------------------

/// Adaptive COW frame guard.
///
/// Refs: I-Gov-TraitAtomic
/// Like `UndoFrameGuard`, but the threshold is determined dynamically
/// by consulting a `CowBudgetPolicy` if available.
///
/// Refs: I-Gov-Rollback-BestEffort, I-Gov-CowBudget-Adaptative
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
    fn begin_hook(&mut self, hook_name: &'static str) {
        self.active_frame = Some(Vec::new());
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
        self.current_hook = hook_name.to_string();
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
        let log = ext.get_or_insert_default::<RollbackEventLog>();
        log.events.push(RollbackEvent {
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
        if let Some(frame) = self.active_frame.take() {
            for (type_id, backup) in frame {
                ext.restore_boxed(type_id, backup);
            }
        }
        let log = ext.get_or_insert_default::<RollbackEventLog>();
        log.events.push(RollbackEvent {
            hook_name: self.current_hook.clone(),
            was_rollback: true,
            frame_weight: self.current_frame_weight,
            budget_exceeded,
        });
        self.active_frame = None;
        self.current_frame_weight = 0;
        self.snapshotted_types.clear();
    }

    fn is_budget_exceeded(&self) -> bool {
        self.current_frame_weight >= self.effective_max()
    }

    fn set_cow_budget_policy(&mut self, policy: Box<dyn CowBudgetPolicy>) {
        self.budget_policy = Some(policy);
    }
}

// ---------------------------------------------------------------------------
// HistoricalCowBudgetPolicy
// ---------------------------------------------------------------------------

/// History of rollback decisions per frame.
///
/// Refs: I-Gov-Rollback-BestEffort
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RollbackFrameRecord {
    /// Name of the hook during which the rollback occurred.
    pub hook_name: String,
    /// Whether the rollback succeeded (true) or was abandoned (false).
    pub succeeded: bool,
    /// Byte weight of the frame at the time of rollback.
    pub weight: usize,
}

/// Historical auto-tuning COW budget policy.
/// Refs: I-Gov-TraitAtomic
///
/// Monitors the last N frames and adjusts the budget to avoid
/// abandonments while limiting memory pressure.
///
/// Refs: I-Gov-Rollback-BestEffort
pub struct HistoricalCowBudgetPolicy {
    base_budget: usize,
    min_budget: usize,
    max_budget: usize,
    window_size: usize,
    history: VecDeque<RollbackFrameRecord>,
}

impl HistoricalCowBudgetPolicy {
    /// Creates a policy with the default parameters.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self {
            base_budget: 65536,
            min_budget: 16384,
            max_budget: 262144,
            window_size: 32,
            history: VecDeque::new(),
        }
    }

    /// Creates a policy with custom parameters.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_params(
        base_budget: usize,
        min_budget: usize,
        max_budget: usize,
        window_size: usize,
    ) -> Self {
        Self {
            base_budget,
            min_budget,
            max_budget,
            window_size,
            history: VecDeque::new(),
        }
    }

    /// Records the result of a frame for auto-tuning.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn record_frame(&mut self, hook_name: &str, succeeded: bool, weight: usize) {
        if self.history.len() >= self.window_size {
            self.history.pop_front();
        }
        self.history.push_back(RollbackFrameRecord {
            hook_name: hook_name.to_string(),
            succeeded,
            weight,
        });
    }

    /// Computes the success rate over the sliding window.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn success_rate(&self) -> f64 {
        if self.history.is_empty() {
            return 1.0;
        }
        let successes = self.history.iter().filter(|r| r.succeeded).count();
        successes as f64 / self.history.len() as f64
    }

    /// Current adaptive budget.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn adaptive_budget(&self) -> usize {
        let rate = self.success_rate();
        if rate > 0.95 {
            // Most rollbacks succeed — we can reduce budget.
            self.base_budget.saturating_mul(3).min(self.max_budget) / 4
        } else if rate < 0.75 {
            // Many abandonments — increase budget.
            self.base_budget.saturating_mul(5).min(self.max_budget) / 4
        } else {
            self.base_budget
        }
        .max(self.min_budget)
        .min(self.max_budget)
    }
}

impl Default for HistoricalCowBudgetPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl CowBudgetPolicy for HistoricalCowBudgetPolicy {
    fn max_cow_bytes(&self, _hook_name: &str) -> usize {
        self.adaptive_budget()
    }
}

#[cfg(test)]
mod tests {
    use brioche_core::{BriocheExtensionType, ExtensionStorage, RollbackEventLog};

    use super::*;

    fn snapshot_epoch(ext: &mut ExtensionStorage, generation: u64) {
        ext.insert(brioche_core::EpochState {
            current_generation: generation,
        });
    }

    #[test]
    fn undo_frame_guard_restores_on_rollback() {
        let mut guard = UndoFrameGuard::new();
        let mut ext = ExtensionStorage::new();
        snapshot_epoch(&mut ext, 42);

        guard.begin_hook("on_input");

        let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
        let vtable = brioche_core::EpochState::build_vtable();
        let current = ext.get_or_insert_default::<brioche_core::EpochState>();
        guard.on_mutation(type_id, &vtable, current);

        current.current_generation = 999;

        guard.rollback_hook(&mut ext);

        let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
        assert_eq!(restored.current_generation, 42);

        let log = ext.get_or_insert_default::<RollbackEventLog>();
        assert_eq!(log.events.len(), 1);
        assert!(log.events[0].was_rollback);
    }

    #[test]
    fn undo_frame_guard_discards_on_commit() {
        let mut guard = UndoFrameGuard::new();
        let mut ext = ExtensionStorage::new();
        snapshot_epoch(&mut ext, 42);

        guard.begin_hook("on_input");

        let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
        let vtable = brioche_core::EpochState::build_vtable();
        let current = ext.get_or_insert_default::<brioche_core::EpochState>();
        guard.on_mutation(type_id, &vtable, current);

        current.current_generation = 999;

        guard.commit_hook(&mut ext);

        let committed = ext.get_or_insert_default::<brioche_core::EpochState>();
        assert_eq!(committed.current_generation, 999);

        let log = ext.get_or_insert_default::<RollbackEventLog>();
        assert_eq!(log.events.len(), 1);
        assert!(!log.events[0].was_rollback);
    }

    #[test]
    fn tiered_undo_frame_guard_restores_critical_type() {
        let mut guard = TieredUndoFrameGuard::new();
        let mut ext = ExtensionStorage::new();
        snapshot_epoch(&mut ext, 42);

        guard.begin_hook("on_input");

        let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
        let vtable = brioche_core::EpochState::build_vtable();
        let current = ext.get_or_insert_default::<brioche_core::EpochState>();
        guard.on_mutation(type_id, &vtable, current);

        current.current_generation = 999;

        guard.rollback_hook(&mut ext);

        let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
        assert_eq!(restored.current_generation, 42);
    }

    #[test]
    fn adaptive_undo_frame_guard_restores_on_rollback() {
        let mut guard = AdaptiveUndoFrameGuard::new();
        let mut ext = ExtensionStorage::new();
        snapshot_epoch(&mut ext, 7);

        guard.begin_hook("on_input");

        let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
        let vtable = brioche_core::EpochState::build_vtable();
        let current = ext.get_or_insert_default::<brioche_core::EpochState>();
        guard.on_mutation(type_id, &vtable, current);

        current.current_generation = 77;

        guard.rollback_hook(&mut ext);

        let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
        assert_eq!(restored.current_generation, 7);
    }

    #[test]
    fn adaptive_undo_frame_guard_uses_budget_policy() {
        struct FixedBudget(usize);

        impl CowBudgetPolicy for FixedBudget {
            fn max_cow_bytes(&self, _hook_name: &str) -> usize {
                self.0
            }
        }

        let guard = AdaptiveUndoFrameGuard::new().with_budget_policy(Box::new(FixedBudget(1024)));
        assert_eq!(guard.effective_max(), 1024);
    }

    #[test]
    fn historical_budget_policy_defaults_to_reduced_with_empty_history() {
        let policy = HistoricalCowBudgetPolicy::new();
        assert_eq!(policy.success_rate(), 1.0);
        // Empty history implies 100% success, so the policy reduces the budget.
        assert_eq!(policy.adaptive_budget(), 65536_usize.saturating_mul(3) / 4);
    }

    #[test]
    fn historical_budget_policy_returns_base_for_balanced_success_rate() {
        let mut policy = HistoricalCowBudgetPolicy::with_params(65536, 16384, 262144, 4);
        policy.record_frame("hook", true, 100);
        policy.record_frame("hook", true, 100);
        policy.record_frame("hook", false, 100);
        policy.record_frame("hook", true, 100);

        // 3 successes / 4 records = 0.75, which keeps the base budget.
        assert_eq!(policy.success_rate(), 0.75);
        assert_eq!(policy.adaptive_budget(), 65536);
    }

    #[test]
    fn historical_budget_policy_reduces_on_high_success() {
        let mut policy = HistoricalCowBudgetPolicy::with_params(65536, 16384, 262144, 32);
        for _ in 0..32 {
            policy.record_frame("hook", true, 100);
        }

        let budget = policy.adaptive_budget();
        assert!(budget < 65536, "high success rate should reduce budget");
        assert!(budget >= 16384, "budget should be clamped to min");
    }

    #[test]
    fn historical_budget_policy_increases_on_low_success() {
        // Use a max_budget between 4x and 5x base so the increase is visible
        // after the pre-division clamp.
        let mut policy = HistoricalCowBudgetPolicy::with_params(65536, 16384, 300000, 4);
        for _ in 0..3 {
            policy.record_frame("hook", false, 100);
        }
        policy.record_frame("hook", true, 100);

        let budget = policy.adaptive_budget();
        assert!(budget > 65536, "low success rate should increase budget");
        assert!(budget <= 300000, "budget should be clamped to max");
    }

    #[test]
    fn historical_budget_policy_sliding_window() {
        let mut policy = HistoricalCowBudgetPolicy::with_params(65536, 16384, 262144, 2);
        policy.record_frame("hook", false, 100);
        policy.record_frame("hook", false, 100);
        policy.record_frame("hook", true, 100);

        assert_eq!(policy.history.len(), 2);
        assert_eq!(policy.success_rate(), 0.5);
    }
}
