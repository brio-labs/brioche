//! ToolCallDetector — Book II §5.5.
//!
//! Detects `ToolCallStart` stream events and manages the transition
//! from `Predicting` to `ExecutingTools`. In the current architecture,
//! the kernel itself handles `StreamToolAccumulator` and state push;
//! this plugin provides a policy hook for observation and telemetry.
//!
//! Refs: I-Core-ActiveToolCall

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, StreamAction, StreamEvent,
};

/// Compteur d'appels d'outils détectés.
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
pub struct ToolCallDetectorState {
    pub total_detected: u64,
    pub total_completed: u64,
}

/// Détecteur d'appels d'outils dans le stream.
///
/// Sur `on_stream_event`, incrémente les compteurs lors des événements
/// `ToolCallStart` et `ToolCallDone`.
pub struct ToolCallDetector;

impl ToolCallDetector {
    /// Crée une nouvelle instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolCallDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for ToolCallDetector {
    fn name(&self) -> &'static str {
        "tool_call_detector"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_STREAM_EVENT
    }

    fn priority(&self) -> i16 {
        10 // Early stream observer
    }

    /// Compte les événements `ToolCallStart` et `ToolCallDone`.
    ///
    /// # Complexity
    /// O(1). Une lecture `ExtensionStorage` + incrément entier.
    fn on_stream_event(
        &self,
        event: &StreamEvent,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        let state = ext.get_or_insert_default::<ToolCallDetectorState>();

        match event {
            StreamEvent::ToolCallStart { .. } => {
                state.total_detected += 1;
            }
            StreamEvent::ToolCallDone { .. } => {
                state.total_completed += 1;
            }
            _ => {}
        }

        Ok(StreamAction::Pass)
    }
}
