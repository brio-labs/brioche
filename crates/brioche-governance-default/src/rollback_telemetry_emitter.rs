//! RollbackTelemetryEmitter — Book II §5.25.
//!
//! Passive observer of abandoned COW rollbacks.
//!
//! On `after_prediction`, inspects the rollback state and emits
//! telemetry metrics (in a full implementation
//! with shell, these metrics would be archived via a dedicated channel).
//!
//! Refs: I-Gov-Rollback-BestEffort

use brioche_core::{BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult};
use std::collections::BTreeMap;

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
    /// Total number of abandoned rollbacks.
    pub abandoned_count: u64,
    /// Total number of successful rollbacks.
    pub restored_count: u64,
    /// Cumulative weight of abandonments (bytes).
    pub abandoned_weight_total: u64,
    /// Map hook_name -> (abandonments, restorations).
    pub per_hook_stats: BTreeMap<String, (u64, u64)>,
}

/// Rollback telemetry emitter.
///
/// Passive plugin that counts COW rollback events.
pub struct RollbackTelemetryEmitter;

impl RollbackTelemetryEmitter {
    /// Creates a new instance.
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
        let _state = ext.get_or_insert_default::<RollbackTelemetryState>();

        // In a full implementation with shell integration, this plugin
        // would observe signals from the CycleRollbackPolicy and update
        // counters. For Sprint 8, we ensure the state structure is present.
        Ok(())
    }
}
