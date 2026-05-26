//! CycleBudgetGuard — implémentation `CycleBudgetPolicy` (Book II §2.6).
//!
//! Définit un budget synchrone par plugin et enregistre les dépassements
//! dans `ExtensionStorage`.
//!
//! Refs: SPECS.md §1.3 (CycleBudgetPolicy)

use brioche_core::{CycleBudgetPolicy, ExtensionStorage};
use std::collections::BTreeMap;

/// État persistant du garde de budget.
///
/// Stocké dans `ExtensionStorage` sous la clé dérivée du type.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
#[brioche(critical_state)]
pub struct CycleBudgetGuardState {
    /// plugin_name → budget_us (0 = non surveillé).
    pub budgets: BTreeMap<String, u64>,
    /// Plugins ayant dépassé leur budget lors du dernier cycle.
    #[brioche(deterministic_order)]
    pub violations: Vec<CycleBudgetViolation>,
}

/// Enregistrement d'une violation de budget.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CycleBudgetViolation {
    pub plugin_name: String,
    pub elapsed_us: u64,
    pub budget_us: u64,
}

/// Garde de budget synchrone par plugin.
///
/// Le kernel consulte ce policy avant chaque hook monitoré. Si le budget
/// est dépassé, le kernel émet `Effect::PluginFault` (Soft) et appelle
/// `on_budget_exceeded` pour persister la trace.
///
/// # Exemple
/// ```
/// use brioche_governance_default::CycleBudgetGuard;
///
/// let guard = CycleBudgetGuard::new()
///     .with_budget("my_plugin", 1000); // 1 ms
/// ```
pub struct CycleBudgetGuard {
    budgets: BTreeMap<String, u64>,
}

impl CycleBudgetGuard {
    /// Crée un garde avec aucun budget défini.
    pub fn new() -> Self {
        Self {
            budgets: BTreeMap::new(),
        }
    }

    /// Définit le budget (en microsecondes) pour un plugin donné.
    pub fn with_budget(mut self, plugin_name: impl Into<String>, budget_us: u64) -> Self {
        self.budgets.insert(plugin_name.into(), budget_us);
        self
    }
}

impl Default for CycleBudgetGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl CycleBudgetPolicy for CycleBudgetGuard {
    fn get_budget(&self, plugin_name: &str) -> u64 {
        self.budgets.get(plugin_name).copied().unwrap_or(0)
    }

    fn on_budget_exceeded(&self, plugin_name: &str, elapsed_us: u64, ext: &mut ExtensionStorage) {
        let state = ext.get_or_insert_default::<CycleBudgetGuardState>();
        let budget_us = self.get_budget(plugin_name);
        state.violations.push(CycleBudgetViolation {
            plugin_name: plugin_name.to_string(),
            elapsed_us,
            budget_us,
        });
    }
}
