//! ContextOptimizer — Book IV §1.3.
//!
//! Monitors history size before prediction and triggers
//! `TriggerSummarization` when a configurable threshold is exceeded.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use brioche_core::{
    BriocheExtensionType, BriochePlugin, ChatMessage, Effect, ExtensionStorage, PluginCapabilities,
    PluginResult, PolicyDecision,
};

/// Context optimizer state.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct ContextOptimizerState {
    /// Maximum desired messages before summarization.
    pub max_messages: usize,
    /// Threshold percentage (0–100) at which to trigger summarization.
    pub threshold_percent: u8,
    /// Number of times summarization has been triggered.
    pub summarizations_triggered: u64,
}

/// Context optimizer plugin.
///
/// Requests `TriggerSummarization` when history length exceeds
/// `max_messages * threshold_percent / 100`.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct ContextOptimizer {
    max_messages: usize,
    threshold_percent: u8,
}

impl ContextOptimizer {
    /// Creates an optimizer with a message limit and threshold.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_threshold(max_messages: usize, threshold_percent: u8) -> Self {
        Self {
            max_messages,
            threshold_percent: threshold_percent.min(100),
        }
    }
}

impl Default for ContextOptimizer {
    fn default() -> Self {
        Self::with_threshold(100, 85)
    }
}

impl BriochePlugin for ContextOptimizer {
    fn name(&self) -> &'static str {
        "context_optimizer"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::BEFORE_PREDICTION
    }

    fn priority(&self) -> i16 {
        -5 // After interceptors, before prediction
    }

    /// Triggers summarization if history exceeds the threshold.
    ///
    /// # Complexity
    /// O(1). Only checks history length.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn before_prediction(
        &self,
        history: &[ChatMessage],
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let should_trigger = ext.with_or_insert_default::<ContextOptimizerState, _>(|state| {
            state.max_messages = self.max_messages;
            state.threshold_percent = self.threshold_percent;

            let threshold = (self.max_messages * self.threshold_percent as usize) / 100;
            if threshold > 0 && history.len() >= threshold {
                state.summarizations_triggered += 1;
                true
            } else {
                false
            }
        });

        if should_trigger {
            return Ok(PolicyDecision::RequestEffect(Effect::TriggerSummarization));
        }

        Ok(PolicyDecision::Allow)
    }
}
