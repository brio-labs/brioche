//! CircuitBreaker — Book IV §1.1.
//!
//! Detects redundant tool call loops by inspecting session history
//! during `before_prediction`. If the same tool with identical arguments
//! appears more than `max_repetitions` consecutively, the plugin returns
//! `Block` to break the loop.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use brioche_core::{
    BriocheExtensionType, BriochePlugin, ChatMessage, ExtensionStorage, PluginCapabilities,
    PluginResult, PolicyDecision,
};

/// Circuit breaker state.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct CircuitBreakerState {
    /// Maximum allowed repetitions of the same tool call.
    pub max_repetitions: u64,
    /// Number of consecutive identical tool calls currently observed.
    pub current_repetitions: u64,
    /// Last observed tool signature (tool_name + arguments).
    pub last_signature: String,
    /// Total number of loops broken.
    pub loops_broken: u64,
}

/// Circuit breaker plugin.
///
/// Prevents infinite tool call loops by blocking predictions when
/// the same tool+arguments pattern repeats excessively.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct CircuitBreaker {
    max_repetitions: u64,
}

impl CircuitBreaker {
    /// Creates a breaker with a repetition limit.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_max_repetitions(max_repetitions: u64) -> Self {
        Self { max_repetitions }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::with_max_repetitions(5)
    }
}

impl BriochePlugin for CircuitBreaker {
    fn name(&self) -> &'static str {
        "circuit_breaker"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::BEFORE_PREDICTION
    }

    fn priority(&self) -> i16 {
        -20 // Early — block before expensive prediction
    }

    /// Scans history for consecutive identical tool calls.
    ///
    /// # Complexity
    /// O(h) where h = history length. One linear scan.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn before_prediction(
        &self,
        history: &[ChatMessage],
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let state = ext.get_or_insert_default::<CircuitBreakerState>();
        state.max_repetitions = self.max_repetitions;

        // Extract tool request signatures from history.
        let mut signatures: Vec<String> = Vec::new();
        for msg in history {
            if let ChatMessage::ToolRequest {
                name, arguments, ..
            } = msg
            {
                signatures.push(format!("{}:{}", name, arguments));
            }
        }

        if signatures.is_empty() {
            state.current_repetitions = 0;
            state.last_signature.clear();
            return Ok(PolicyDecision::Allow);
        }

        // Count consecutive identical signatures from the end.
        let last = signatures.last().cloned().unwrap_or_default();
        let mut consecutive = 1u64;
        for i in (0..signatures.len() - 1).rev() {
            if signatures[i] == last {
                consecutive += 1;
            } else {
                break;
            }
        }

        state.current_repetitions = consecutive;
        state.last_signature = last.clone();

        if consecutive > self.max_repetitions {
            state.loops_broken += 1;
            return Ok(PolicyDecision::Block {
                reason: format!(
                    "circuit breaker: tool call loop detected ({} consecutive repetitions of {})",
                    consecutive, last
                ),
            });
        }

        Ok(PolicyDecision::Allow)
    }
}
