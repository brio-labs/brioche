//! PendingTaskManager — Book IV §1.6.
//!
//! Manages long-running tasks via the `Pending` pattern.
//! On `on_tool_result`, detects tasks that indicate they are pending
//! and stores them. Checks `SignalBuffer` for async status updates.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use std::collections::BTreeMap;

use brioche_core::{
    AsyncTaskResult, BriocheExtensionType, EngineInput, ExtensionStorage, OnInput, OnToolResult,
    PluginResult, PolicyDecision, SignalBuffer, ToolResultDTO,
};

use crate::Priority;

/// Marker string that a tool result must contain to be treated as pending.
const PENDING_MARKER: &str = "__PENDING__";

/// Information about a pending task.
///
/// ## Snapshot strategy
/// COW: full clone (~64 bytes). Two `String` fields plus one enum.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct PendingTaskInfo {
    /// Identifier of the pending task.
    pub task_id: String,
    /// How long to wait before the next status check (milliseconds).
    pub check_after_ms: u64,
    /// Current status of the task.
    pub status: PendingTaskStatus,
}

/// Status of a pending task.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub enum PendingTaskStatus {
    /// Task is queued but not yet started.
    #[default]
    Pending,
    /// Task is currently executing.
    Running,
    /// Task completed successfully.
    Completed(String),
    /// Task failed with an error message.
    Failed(String),
}

/// Pending task manager state.
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with pending tasks (typically < 10).
/// One `BTreeMap` plus one counter.
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

impl OnToolResult for PendingTaskManager {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type ToolResultDto = ToolResultDTO;

    fn name(&self) -> &'static str {
        "pending_task_manager"
    }

    fn priority(&self) -> i16 {
        Priority::PENDING_TASK // After primary processors
    }

    /// Inspects tool results for pending task indicators.
    ///
    /// If a result contains `PENDING_MARKER` (`"__PENDING__"`), it is treated as
    /// a long-running task handle and stored for later status checks.
    ///
    /// # Panics
    /// Never panics. No indexing or conditional allocation.
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
                && content.contains(PENDING_MARKER)
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
}

impl OnInput for PendingTaskManager {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "pending_task_manager"
    }

    fn priority(&self) -> i16 {
        Priority::PENDING_TASK
    }

    /// Consumes async task results from the `SignalBuffer`.
    ///
    /// The shell drains `AsyncTaskResult::ToolStatusCheck` events into
    /// `ExtensionStorage` as `SignalBuffer` before each transition.
    ///
    /// # Panics
    /// Never panics. All collection access is via safe `BTreeMap` APIs.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn on_input(
        &self,
        _input: &EngineInput,
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
                                } => {
                                    let out = partial_output.clone();
                                    out.map_or(String::new(), |v| v)
                                }
                                _ => String::new(),
                            };
                            PendingTaskStatus::Completed(msg)
                        }
                        _ => PendingTaskStatus::Pending,
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
