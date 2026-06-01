//! HistoricalCowBudgetPolicy — Book II §2.11, §5.26.
//!
//! Sliding-window auto-tuning of COW budget based on historical rollback
//! success/failure rates.
//!
//! Refs: I-Gov-CowBudget-Adaptative

use brioche_core::CowBudgetPolicy;
use std::collections::VecDeque;

/// History of rollback decisions per frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RollbackFrameRecord {
    pub hook_name: String,
    pub succeeded: bool,
    pub weight: usize,
}

/// Historical auto-tuning COW budget policy.
///
/// Monitors the last N frames and adjusts the budget to avoid
/// abandonments while limiting memory pressure.
pub struct HistoricalCowBudgetPolicy {
    base_budget: usize,
    min_budget: usize,
    max_budget: usize,
    window_size: usize,
    history: VecDeque<RollbackFrameRecord>,
}

impl HistoricalCowBudgetPolicy {
    /// Creates a policy with the default parameters.
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
    pub fn success_rate(&self) -> f64 {
        if self.history.is_empty() {
            return 1.0;
        }
        let successes = self.history.iter().filter(|r| r.succeeded).count();
        successes as f64 / self.history.len() as f64
    }

    /// Budget adaptatif courant.
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
