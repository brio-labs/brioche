//! TokenTracker — Book IV §1.2.
//!
//! Estimates real-time token volume by inspecting history length.
//! Uses a deterministic heuristic (character count / 4) so that token
//! estimates are replay-stable.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use brioche_core::{
    BriocheExtensionType, BriochePlugin, ChatMessage, ExtensionStorage, PluginCapabilities,
    PluginResult,
};
use std::collections::BTreeMap;

/// Token tracking state.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct TokenTrackerState {
    /// Estimated total input tokens (user + system + tool results).
    pub total_input_tokens: u64,
    /// Estimated total output tokens (assistant text + tool requests).
    pub total_output_tokens: u64,
    /// Number of prediction cycles observed.
    pub prediction_cycles: u64,
    /// Per-message-type token counts for telemetry.
    pub tokens_by_role: BTreeMap<String, u64>,
    /// Output tokens estimated during the current cycle (buffered).
    pub buffered_output_tokens: u64,
}

/// Token volume tracker.
///
/// Provides deterministic token estimates without calling external
/// tokenizers. The heuristic is intentionally simple to preserve
/// replay stability: `ceil(len / 4)` per message content.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct TokenTracker;

impl TokenTracker {
    /// Creates a new instance.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn new() -> Self {
        Self
    }

    /// Estimate tokens for a single string.
    ///
    /// Complexity: O(1). Integer division.
    fn estimate_tokens(text: &str) -> u64 {
        (text.len() as u64).div_ceil(4)
    }

    /// Estimate tokens for a `ChatMessage`.
    fn estimate_message_tokens(msg: &ChatMessage) -> (u64, &'static str) {
        match msg {
            ChatMessage::System { content } => (Self::estimate_tokens(content), "system"),
            ChatMessage::User { content } => (Self::estimate_tokens(content), "user"),
            ChatMessage::Assistant { content } => (Self::estimate_tokens(content), "assistant"),
            ChatMessage::ToolRequest { arguments, .. } => {
                (Self::estimate_tokens(arguments), "tool_request")
            }
            ChatMessage::ToolResult { outcome, .. } => {
                (Self::estimate_tokens(outcome.content()), "tool_result")
            }
            _ => (0, "unknown"),
        }
    }
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for TokenTracker {
    fn name(&self) -> &'static str {
        "token_tracker"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::BEFORE_PREDICTION | PluginCapabilities::AFTER_PREDICTION
    }

    fn priority(&self) -> i16 {
        60 // Late observer — let interceptors run first
    }

    /// Computes input token estimates from history before prediction.
    ///
    /// # Complexity
    /// O(h · log r) where h = history length, r = number of roles.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn before_prediction(
        &self,
        history: &[ChatMessage],
        ext: &mut ExtensionStorage,
    ) -> PluginResult<brioche_core::PolicyDecision> {
        ext.with_or_insert_default::<TokenTrackerState, _>(|state| {
            // Reset buffered output from previous cycle.
            state.buffered_output_tokens = 0;

            for msg in history {
                let (tokens, role) = Self::estimate_message_tokens(msg);
                *state.tokens_by_role.entry(role.to_string()).or_insert(0) += tokens;

                match msg {
                    ChatMessage::Assistant { .. } | ChatMessage::ToolRequest { .. } => {
                        state.total_output_tokens += tokens;
                        state.buffered_output_tokens += tokens;
                    }
                    _ => {
                        state.total_input_tokens += tokens;
                    }
                }
            }

            Ok(brioche_core::PolicyDecision::Allow)
        })
    }

    /// Finalizes cycle count after prediction completes.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        ext.with_or_insert_default::<TokenTrackerState, _>(|state| {
            state.prediction_cycles += 1;
        });
        Ok(())
    }
}
