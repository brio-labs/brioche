//! RollbackTelemetryEmitter — Book II §5.25.
//!
//! Passive observer of abandoned COW rollbacks.
//!
//! Sur `after_prediction`, inspecte l'état des rollbacks et émet
//! des métriques de télémesure (dans une implémentation complète
//! avec shell, ces métriques seraient archivées via un canal dédié).
//!
//! Refs: I-Gov-Rollback-BestEffort

use brioche_core::{BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult};
use std::collections::BTreeMap;

/// Métriques de rollback observées.
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
    /// Nombre total de rollbacks abandonnés.
    pub abandoned_count: u64,
    /// Nombre total de rollbacks réussis.
    pub restored_count: u64,
    /// Poids cumulé des abandons (octets).
    pub abandoned_weight_total: u64,
    /// Map hook_name -> (abandons, restaurations).
    pub per_hook_stats: BTreeMap<String, (u64, u64)>,
}

/// Émetteur de télémesure de rollback.
///
/// Plugin passif qui comptabilise les événements de rollback COW.
pub struct RollbackTelemetryEmitter;

impl RollbackTelemetryEmitter {
    /// Crée une nouvelle instance.
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
