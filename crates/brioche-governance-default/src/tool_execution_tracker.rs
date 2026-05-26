//! ToolExecutionTracker — plugin de télémétrie d'exécution d'outils (Book II §5.9).
//!
//! Comptabilise les appels d'outils, les succès/échecs et la durée cumulée
//! sans jamais muter l'état mécanique du noyau (`session.active_tools`).
//!
//! Refs: I-Eco-ExtensionOverMod

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, ToolCallDescriptor,
    ToolResultDTO,
};
use std::collections::BTreeMap;

/// État persistant du tracker d'exécution.
///
/// Stocké dans `ExtensionStorage`. Utilise `BTreeMap` pour garantir
/// l'itération déterministe.
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
pub struct ToolExecutionTelemetry {
    /// Nombre d'outils ayant terminé avec succès.
    pub completed_count: u64,
    /// Nombre d'outils ayant échoué (erreur métier ou système).
    pub failed_count: u64,
    /// Durée cumulée d'exécution en millisecondes.
    pub total_duration_ms: u64,
    /// Timestamp de début par tool_id (pour calcul de durée).
    pub start_timestamps: BTreeMap<String, u64>,
}

/// Tracker d'exécution d'outils.
///
/// Enregistre des métriques de haut niveau sur les appels d'outils.
/// Les données sont purement télémetriques ; aucune décision de transition
/// n'est prise par ce plugin.
pub struct ToolExecutionTracker;

impl ToolExecutionTracker {
    /// Crée une nouvelle instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolExecutionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for ToolExecutionTracker {
    fn name(&self) -> &'static str {
        "tool_execution_tracker"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_TOOL_CALLS | PluginCapabilities::ON_TOOL_RESULT
    }

    fn on_tool_calls(
        &self,
        calls: &mut Vec<ToolCallDescriptor>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<ToolExecutionTelemetry>();
        let now = 0u64; // Déterministe : le shell fournira les vrais timestamps via ExtensionStorage si besoin.
        for call in calls {
            state.start_timestamps.insert(call.tool_id.clone(), now);
        }
        Ok(())
    }

    fn on_tool_result(
        &self,
        results: &mut Vec<ToolResultDTO>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<ToolExecutionTelemetry>();
        for result in results {
            match &result.outcome {
                brioche_core::ToolOutcome::Success(_) => {
                    state.completed_count += 1;
                }
                brioche_core::ToolOutcome::BusinessError(_) => {
                    state.failed_count += 1;
                }
                brioche_core::ToolOutcome::SystemError(_) => {
                    state.failed_count += 1;
                }
                brioche_core::ToolOutcome::TimeoutWithPartialData { .. } => {
                    state.failed_count += 1;
                }
            }
            // Retirer le timestamp de début ; la durée est 0 dans ce
            // modèle déterministe (le shell peut enrichir via effet).
            state.start_timestamps.remove(&result.tool_id);
        }
        Ok(())
    }
}
