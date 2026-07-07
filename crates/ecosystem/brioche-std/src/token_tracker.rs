//! TokenTracker — Book IV §1.2.
//!
//! Estimates real-time token volume by inspecting history length.
//! Uses a deterministic heuristic (character count / 4) so that token
//! estimates are replay-stable.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use std::collections::BTreeMap;

use brioche_core::{
    AfterPrediction, BeforePrediction, BriocheExtensionType, ChatMessage, ExtensionStorage,
    PluginResult,
};

use crate::Priority;

/// Token role key for deterministic telemetry.
///
/// The variants mirror the exhaustive `ChatMessage` roles observed by
/// `TokenTracker`, avoiding string allocation for every message.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum TokenRole {
    /// System prompt or instruction.
    System,
    /// User-authored message.
    User,
    /// Assistant-authored response.
    Assistant,
    /// Assistant tool invocation request.
    ToolRequest,
    /// Tool execution result.
    ToolResult,
    /// Future `ChatMessage` variant not known to this crate.
    Unknown,
}

/// Token tracking state.
///
/// ## Snapshot strategy
/// COW: full clone (~32 bytes). Three scalar fields plus one
/// `BTreeMap<TokenRole, u64>` (at most six entries).
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
    pub tokens_by_role: BTreeMap<TokenRole, u64>,
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
    fn estimate_message_tokens(msg: &ChatMessage) -> (u64, TokenRole) {
        match msg {
            ChatMessage::System { content } => (Self::estimate_tokens(content), TokenRole::System),
            ChatMessage::User { content } => (Self::estimate_tokens(content), TokenRole::User),
            ChatMessage::Assistant { content, .. } => {
                (Self::estimate_tokens(content), TokenRole::Assistant)
            }
            ChatMessage::ToolRequest { arguments, .. } => {
                (Self::estimate_tokens(arguments), TokenRole::ToolRequest)
            }
            ChatMessage::ToolResult { content, .. } => {
                (Self::estimate_tokens(content), TokenRole::ToolResult)
            }
            _ => (0, TokenRole::Unknown),
        }
    }
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl BeforePrediction for TokenTracker {
    type ChatMessage = ChatMessage;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = brioche_core::PolicyDecision;

    fn name(&self) -> &'static str {
        "token_tracker"
    }

    fn priority(&self) -> i16 {
        Priority::TOKEN_TRACKER // Late observer — let interceptors run first
    }

    /// Computes input token estimates from history before prediction.
    ///
    /// # Complexity
    /// O(h · log r) where h = history length, r = tracked role count bounded
    /// by `TokenRole` variants. Does not allocate per message for role keys.
    ///
    /// # Panics
    /// Never panics. No indexing; all access is via safe `BTreeMap` APIs.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn before_prediction(
        &self,
        history: &[ChatMessage],
        ext: &mut ExtensionStorage,
    ) -> PluginResult<brioche_core::PolicyDecision> {
        let state = ext.get_or_insert_default::<TokenTrackerState>();

        // Reset buffered output from previous cycle.
        state.buffered_output_tokens = 0;

        for msg in history {
            let (tokens, role) = Self::estimate_message_tokens(msg);
            *state.tokens_by_role.entry(role).or_insert(0) += tokens;

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
    }
}

impl AfterPrediction for TokenTracker {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "token_tracker"
    }

    fn priority(&self) -> i16 {
        Priority::TOKEN_TRACKER
    }

    /// Finalizes cycle count after prediction completes.
    ///
    /// # Panics
    /// Never panics. Scalar increment only.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<TokenTrackerState>();
        state.prediction_cycles += 1;
        Ok(())
    }
}
