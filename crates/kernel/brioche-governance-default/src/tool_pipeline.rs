//! Tool call lifecycle plugins.
//!
//! This module groups plugins that observe or transform the tool
//! execution pipeline:
//! - `ToolResultFormatter`: truncates and formats tool results.
//! - `ToolExecutionTracker`: records tool execution telemetry.
//!
//! Refs: I-Core-ActiveToolCall, I-Eco-ExtensionOverMod

use std::collections::BTreeMap;

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginError, PluginResult,
    ToolCallDescriptor, ToolOutcome, ToolResultDTO, TruncatedToolResult, tool_outcome_to_string,
};

use crate::Priority;

// ---------------------------------------------------------------------------
// ToolResultFormatter
// ---------------------------------------------------------------------------

/// Tool result formatting configuration.
///
/// ## Snapshot strategy
/// COW: full clone (~16 bytes). Two scalars.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
pub struct ToolResultFormatterState {
    /// Maximum size of a JSON result in bytes (0 = no limit).
    pub max_result_bytes: usize,
    /// Total number of formatted results.
    pub formatted_count: u64,
}

impl Default for ToolResultFormatterState {
    fn default() -> Self {
        Self {
            max_result_bytes: 65536,
            formatted_count: 0,
        }
    }
}

/// Tool result formatter.
///
/// On `on_tool_result`, formats and truncates results if necessary.
///
/// Refs: I-Core-ActiveToolCall
pub struct ToolResultFormatter {
    max_result_bytes: usize,
}

impl ToolResultFormatter {
    /// Creates a new instance with default values.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self {
            max_result_bytes: 65536,
        }
    }

    /// Creates an instance with a custom size limit.
    /// Refs: I-Gov-TraitAtomic
    pub fn with_max_result_bytes(max_result_bytes: usize) -> Self {
        Self { max_result_bytes }
    }
}

impl Default for ToolResultFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for ToolResultFormatter {
    fn name(&self) -> &'static str {
        "tool_result_formatter"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_TOOL_RESULT
    }

    fn priority(&self) -> i16 {
        Priority::TOOL_FORMATTER // Early formatter — apply limits before other plugins inspect
    }

    fn on_tool_result(
        &self,
        results: &mut Vec<ToolResultDTO>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<ToolResultFormatterState>();
        state.max_result_bytes = self.max_result_bytes;

        for result in results {
            let content = tool_outcome_to_string(&result.outcome);

            if self.max_result_bytes > 0 && content.len() > self.max_result_bytes {
                let meta = TruncatedToolResult::from_content(&content, self.max_result_bytes);
                let json = meta.to_json().map_err(|e| PluginError::Soft {
                    plugin_name: "tool_result_formatter".into(),
                    message: format!("JSON serialization failed: {e}"),
                })?;
                result.outcome = ToolOutcome::Success(json);
            }

            state.formatted_count += 1;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ToolExecutionTracker
// ---------------------------------------------------------------------------

/// Persistent execution tracker state.
///
/// Stored in `ExtensionStorage`. Uses `BTreeMap` to guarantee
/// deterministic iteration.
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with number of in-flight tools
/// (typically < 10). One `BTreeMap<String, u64>` plus three counters.
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
    /// Total number of tool calls observed (including pending).
    pub total_calls: u64,
}

/// Tool execution tracker.
///
/// Records high-level metrics on tool calls.
/// The data is purely telemetry; no transition decision
/// is made by this plugin.
/// Refs: I-Gov-TraitAtomic
///
/// Refs: I-Core-ActiveToolCall, I-Eco-ExtensionOverMod
pub struct ToolExecutionTracker;

impl ToolExecutionTracker {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
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
                ToolOutcome::Success(_) => {
                    state.completed_count += 1;
                }
                ToolOutcome::BusinessError(_) => {
                    state.failed_count += 1;
                }
                ToolOutcome::SystemError(_) => {
                    state.failed_count += 1;
                }
                ToolOutcome::TimeoutWithPartialData { .. } => {
                    state.failed_count += 1;
                }
                _ => {}
            }
            // Remove the start timestamp; duration is 0 in this
            // deterministic model (the shell may enrich via effect).
            state.start_timestamps.remove(&result.tool_id);
        }
        Ok(())
    }
}
