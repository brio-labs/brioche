//! ToolExecutionTracker — tool execution telemetry plugin (Book II §5.9).
//!
//! Counts tool calls, successes/failures and cumulative duration
//! without ever mutating the mechanical state of the kernel (`session.active_tools`).
//!
//! Refs: I-Eco-ExtensionOverMod

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, ToolCallDescriptor,
    ToolResultDTO,
};
use std::collections::BTreeMap;

/// Persistent execution tracker state.
///
/// Stored in `ExtensionStorage`. Uses `BTreeMap` to guarantee
/// deterministic iteration.
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
    /// Number of tools that completed successfully.
    pub completed_count: u64,
    /// Number of tools that failed (business or system error).
    pub failed_count: u64,
    /// Cumulative execution duration in milliseconds.
    pub total_duration_ms: u64,
    /// Start timestamp per tool_id (for duration calculation).
    pub start_timestamps: BTreeMap<String, u64>,
}

/// Tool execution tracker.
///
/// Records high-level metrics on tool calls.
/// The data is purely telemetry; no transition decision
/// is made by this plugin.
pub struct ToolExecutionTracker;

impl ToolExecutionTracker {
    /// Creates a new instance.
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

    /// Records start timestamps for each call.
    ///
    /// # Complexity
    /// O(c · log n). `c` calls; one `BTreeMap` insertion per call.
    fn on_tool_calls(
        &self,
        calls: &mut Vec<ToolCallDescriptor>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<ToolExecutionTelemetry>();
        let now = 0u64; // Deterministic: the shell will provide real timestamps via ExtensionStorage if needed.
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
            // Remove the start timestamp; duration is 0 in this
            // deterministic model (the shell may enrich via effect).
            state.start_timestamps.remove(&result.tool_id);
        }
        Ok(())
    }
}
