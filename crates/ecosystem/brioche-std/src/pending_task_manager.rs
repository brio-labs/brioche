//! PendingTaskManager — Book IV §1.6.
//!
//! Manages long-running tasks via the `Pending` pattern.
//! On `on_tool_result`, detects tasks that indicate they are pending
//! and stores them. Checks `SignalBuffer` for async status updates.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use brioche_core::{
    AsyncTaskResult, BriocheExtensionType, BriochePlugin, ExtensionStorage, PluginCapabilities,
    PluginResult, PolicyDecision, SignalBuffer, ToolResultDTO,
};
use std::collections::BTreeMap;

/// Information about a pending task.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingTaskInfo {
    pub task_id: String,
    pub check_after_ms: u64,
    pub status: PendingTaskStatus,
}

/// Status of a pending task.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PendingTaskStatus {
    #[default]
    Pending,
    Running,
    Completed(String),
    Failed(String),
}

/// Pending task manager state.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct PendingTaskState {
    /// Map task_id -> pending task info.
    pub pending: BTreeMap<String, PendingTaskInfo>,
    /// Default check interval in milliseconds.
    pub default_check_after_ms: u64,
}

/// Pending task manager.
///
/// Detects long-running tool results and consumes async status checks
/// from the `SignalBuffer` injected by the shell.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct PendingTaskManager {
    default_check_after_ms: u64,
}

impl PendingTaskManager {
    /// Creates a manager with a default check interval.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_default_check_after_ms(default_check_after_ms: u64) -> Self {
        Self {
            default_check_after_ms,
        }
    }
}

impl Default for PendingTaskManager {
    fn default() -> Self {
        Self::with_default_check_after_ms(5000)
    }
}

impl BriochePlugin for PendingTaskManager {
    fn name(&self) -> &'static str {
        "pending_task_manager"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_TOOL_RESULT | PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        20 // After primary processors
    }

    /// Inspects tool results for pending task indicators.
    ///
    /// If a result contains the string `"__PENDING__"`, it is treated as
    /// a long-running task handle and stored for later status checks.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn on_tool_result(
        &self,
        results: &mut Vec<ToolResultDTO>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<PendingTaskState>();
        state.default_check_after_ms = self.default_check_after_ms;

        for result in results {
            if let brioche_core::ToolOutcome::Success(content) = &result.outcome
                && content.contains("__PENDING__")
            {
                let info = PendingTaskInfo {
                    task_id: result.tool_id.clone(),
                    check_after_ms: self.default_check_after_ms,
                    status: PendingTaskStatus::Pending,
                };
                state.pending.insert(result.tool_id.clone(), info);
            }
        }

        Ok(())
    }

    /// Consumes async task results from the `SignalBuffer`.
    ///
    /// The shell drains `AsyncTaskResult::ToolStatusCheck` events into
    /// `ExtensionStorage` as `SignalBuffer` before each transition.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn on_input(
        &self,
        _input: &brioche_core::EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let buffer = ext.get_or_insert_default::<SignalBuffer>();

        // Collect status updates without holding a mutable borrow across updates.
        let updates: Vec<(String, PendingTaskStatus)> = buffer
            .async_task_results
            .iter()
            .filter_map(|ar| match ar {
                AsyncTaskResult::ToolStatusCheck { task_id, status } => {
                    let new_status = match status {
                        brioche_core::ToolStatus::Running => PendingTaskStatus::Running,
                        brioche_core::ToolStatus::Completed(outcome) => {
                            let msg = match outcome {
                                brioche_core::ToolOutcome::Success(s)
                                | brioche_core::ToolOutcome::BusinessError(s)
                                | brioche_core::ToolOutcome::SystemError(s) => s.clone(),
                                brioche_core::ToolOutcome::TimeoutWithPartialData {
                                    partial_output,
                                } => partial_output.clone().unwrap_or_default(),
                                _ => String::new(),
                            };
                            PendingTaskStatus::Completed(msg)
                        }
                    };
                    Some((task_id.clone(), new_status))
                }
                _ => None,
            })
            .collect();

        if !updates.is_empty() {
            let state = ext.get_or_insert_default::<PendingTaskState>();
            for (task_id, status) in updates {
                if let Some(info) = state.pending.get_mut(&task_id) {
                    info.status = status;
                }
            }
        }

        Ok(PolicyDecision::Allow)
    }
}
