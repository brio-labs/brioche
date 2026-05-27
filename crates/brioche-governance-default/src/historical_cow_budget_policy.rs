//! HistoricalCowBudgetPolicy — Book II §2.11, §5.26.
//!
//! Sliding-window auto-tuning of COW budget based on historical rollback
//! success/failure rates.
//!
//! Refs: I-Gov-CowBudget-Adaptative

use brioche_core::CowBudgetPolicy;
use std::collections::VecDeque;

/// Historique des décisions de rollback par frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RollbackFrameRecord {
    pub hook_name: String,
    pub succeeded: bool,
    pub weight: usize,
}

/// Politique de budget COW à auto-tuning historique.
///
/// Surveille les N dernières frames et ajuste le budget pour éviter
/// les abandons tout en limitant la pression mémoire.
pub struct HistoricalCowBudgetPolicy {
    base_budget: usize,
    min_budget: usize,
    max_budget: usize,
    window_size: usize,
    history: VecDeque<RollbackFrameRecord>,
}

impl HistoricalCowBudgetPolicy {
    /// Crée une politique avec les paramètres par défaut.
    pub fn new() -> Self {
        Self {
            base_budget: 65536,
            min_budget: 16384,
            max_budget: 262144,
            window_size: 32,
            history: VecDeque::new(),
        }
    }

    /// Crée une politique avec des paramètres personnalisés.
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

    /// Enregistre le résultat d'une frame pour l'auto-tuning.
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

    /// Calcule le taux de succès sur la fenêtre glissante.
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
