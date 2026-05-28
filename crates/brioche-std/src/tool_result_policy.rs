//! ToolResultPolicy — Book IV §1.5.
//!
//! Truncates oversized tool results before they are persisted to history.
//! Similar to `ToolResultFormatter` in governance-default, but provided
//! as a standard ecosystem plugin with opinionated defaults.
//!
//! Refs: I-Eco-ExtensionOverMod

use brioche_core::{
    BriocheExtensionType, BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult,
    ToolResultDTO,
};

/// Tool result policy state.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct ToolResultPolicyState {
    /// Maximum size of a result in bytes (0 = no limit).
    pub max_result_bytes: usize,
    /// Total number of results processed.
    pub results_processed: u64,
    /// Total number of results truncated.
    pub results_truncated: u64,
}

/// Tool result policy plugin.
///
/// On `on_tool_result`, truncates any result whose content exceeds
/// `max_result_bytes`. The truncated result is wrapped in a JSON
/// envelope preserving the original length and a preview.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct ToolResultPolicy {
    max_result_bytes: usize,
}

impl ToolResultPolicy {
    /// Creates a policy with a size limit.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_max_result_bytes(max_result_bytes: usize) -> Self {
        Self { max_result_bytes }
    }
}

impl Default for ToolResultPolicy {
    fn default() -> Self {
        Self::with_max_result_bytes(65536)
    }
}

impl BriochePlugin for ToolResultPolicy {
    fn name(&self) -> &'static str {
        "tool_result_policy"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_TOOL_RESULT
    }

    fn priority(&self) -> i16 {
        5 // Early formatter — apply limits before other plugins inspect
    }

    /// Truncates oversized results.
    ///
    /// # Complexity
    /// O(r) where r = number of results. Linear scan with optional allocation.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn on_tool_result(
        &self,
        results: &mut Vec<ToolResultDTO>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<ToolResultPolicyState>();
        state.max_result_bytes = self.max_result_bytes;

        for result in results {
            let content = match &result.outcome {
                brioche_core::ToolOutcome::Success(s)
                | brioche_core::ToolOutcome::BusinessError(s)
                | brioche_core::ToolOutcome::SystemError(s) => s.clone(),
                brioche_core::ToolOutcome::TimeoutWithPartialData { partial_output } => {
                    partial_output.clone().unwrap_or_default()
                }
            };

            if self.max_result_bytes > 0 && content.len() > self.max_result_bytes {
                let truncated = format!(
                    "{{\"truncated\":true,\"original_len\":{},\"preview\":\"{}\"}}",
                    content.len(),
                    &content[..self.max_result_bytes.min(content.len())]
                );
                result.outcome = brioche_core::ToolOutcome::Success(truncated);
                state.results_truncated += 1;
            }

            state.results_processed += 1;
        }

        Ok(())
    }
}
