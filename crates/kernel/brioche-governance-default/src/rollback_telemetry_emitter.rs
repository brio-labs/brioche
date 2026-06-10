//! RollbackTelemetryEmitter — Book II §5.25.
//!
//! Consumes `RollbackEventLog` produced by `CycleRollbackPolicy`
//! implementations during `commit_hook` and `rollback_hook`, aggregates
//! per-hook statistics, and writes them into `RollbackTelemetryState`.
//!
//! Refs: I-Gov-Rollback-BestEffort, I-Comp-Pure-Logic

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, RollbackEventLog,
};

/// Observed rollback metrics.
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
pub struct RollbackTelemetryState {
    /// Total number of abandoned rollbacks (budget exceeded).
    pub abandoned_count: u64,
    /// Total number of successful rollbacks.
    pub restored_count: u64,
    /// Cumulative weight of abandonments (bytes).
    pub abandoned_weight_total: u64,
    /// Per-hook stats: (hook_name, abandonments, restorations).
    ///
    /// Stored as a `Vec` for cache-locality (n < 20 hooks).
    /// Order is deterministic: insertion order matches hook execution order.
    #[brioche(deterministic_order)]
    pub per_hook_stats: Vec<(String, u64, u64)>,
}

/// Rollback telemetry emitter.
///
/// Reads `RollbackEventLog` from `ExtensionStorage`, aggregates metrics,
/// and stores them in `RollbackTelemetryState`. Clears the log after
/// consumption to avoid double-counting.
///
/// Refs: I-Gov-Rollback-BestEffort
pub struct RollbackTelemetryEmitter;

impl RollbackTelemetryEmitter {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for RollbackTelemetryEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for RollbackTelemetryEmitter {
    fn name(&self) -> &'static str {
        "rollback_telemetry_emitter"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::AFTER_PREDICTION
    }

    fn priority(&self) -> i16 {
        200 // Very late observer
    }

    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        // Steal events to avoid double mutable borrow of ext.
        let events = {
            let log = ext.get_or_insert_default::<RollbackEventLog>();
            std::mem::take(&mut log.events)
        };

        let state = ext.get_or_insert_default::<RollbackTelemetryState>();

        for event in &events {
            if event.was_rollback {
                if event.budget_exceeded {
                    state.abandoned_count += 1;
                    state.abandoned_weight_total += event.frame_weight as u64;
                } else {
                    state.restored_count += 1;
                }
            }

            // Update per-hook stats using linear scan (n < 20).
            let hook_name = event.hook_name.clone();
            if let Some(entry) = state
                .per_hook_stats
                .iter_mut()
                .find(|(name, _, _)| *name == hook_name)
            {
                if event.was_rollback && event.budget_exceeded {
                    entry.1 += 1;
                } else if event.was_rollback {
                    entry.2 += 1;
                }
            } else {
                let abandonments = if event.was_rollback && event.budget_exceeded {
                    1
                } else {
                    0
                };
                let restorations = if event.was_rollback && !event.budget_exceeded {
                    1
                } else {
                    0
                };
                state
                    .per_hook_stats
                    .push((hook_name, abandonments, restorations));
            }
        }

        Ok(())
    }
}
