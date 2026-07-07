//! Telemetry observers — Book II §5.
//!
//! Reference implementations for event tracking and logging:
//! - `TelemetryPlugin`: unified telemetry observer (merged from
//!   `ToolCallDetector`, `TransitionConflictLogger`,
//!   `ToolExecutionTracker`, and `RollbackTelemetryEmitter`)
//!
//! Refs: I-Core-ActiveToolCall, I-Gov-OverrideTrace, I-Gov-Rollback-BestEffort

use brioche_core::{
    AfterPrediction, ExtensionStorage, OnStreamEvent, PluginResult, RollbackEventLog, StreamAction,
    StreamEvent, SupersededTransitionTraceLog,
};

use crate::Priority;

/// Detected tool call counter.
///
/// ## Snapshot strategy
/// COW: full clone (~16 bytes). Two scalar fields.
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
    /// Total tool call starts detected.
    pub total_detected: u64,
    /// Total tool call completions detected.
    pub total_completed: u64,
}

/// Archived transition conflict summary.
///
/// ## Snapshot strategy
/// COW: full clone (~40 bytes). Three scalars + one optional short String.
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
pub struct TransitionConflictState {
    /// Total number of conflicts observed since startup.
    pub total_conflicts: u64,
    /// Number of unique preempted plugins.
    pub unique_preempted_plugins: u64,
    /// Last preempted plugin name (if any).
    pub last_preempted_plugin: Option<String>,
}

/// Observed rollback metrics.
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with number of hooks tracked
/// (typically < 20). One `Vec` of tuples plus four scalar counters.
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
pub struct RollbackTelemetryState {
    /// Total number of abandoned rollbacks (budget exceeded).
    pub abandoned_count: u64,
    /// Total number of successful rollbacks.
    pub restored_count: u64,
    /// Cumulative weight of abandonments (bytes).
    pub abandoned_weight_total: u64,
    /// Per-hook stats: (hook_name, abandonments, restorations).
    ///
    /// Stored as a `Vec` for cache-locality (n < 20 hooks).
    /// Order is deterministic: insertion order matches hook execution order.
    #[brioche(deterministic_order)]
    pub per_hook_stats: Vec<(String, u64, u64)>,
}

// ---------------------------------------------------------------------------
// TelemetryPlugin — unified telemetry observer
// ---------------------------------------------------------------------------

/// Unified telemetry plugin.
///
/// Combines `ToolCallDetector`, `TransitionConflictLogger`,
/// `ToolExecutionTracker`, and `RollbackTelemetryEmitter` into a single plugin.
///
/// Refs: I-Gov-TraitAtomic
pub struct TelemetryPlugin;

impl TelemetryPlugin {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for TelemetryPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl OnStreamEvent for TelemetryPlugin {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type StreamAction = StreamAction;
    type StreamEvent = StreamEvent;

    fn name(&self) -> &'static str {
        "telemetry_plugin"
    }

    fn priority(&self) -> i16 {
        Priority::TELEMETRY
    }

    /// Counts `ToolCallStart` and `ToolCallDone` events.
    ///
    /// # Complexity
    /// O(1). One `ExtensionStorage` read + integer increment.
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

impl AfterPrediction for TelemetryPlugin {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "telemetry_plugin"
    }

    fn priority(&self) -> i16 {
        Priority::TELEMETRY
    }

    /// Archives transition conflicts and aggregates rollback metrics.
    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        // Transition conflict logging
        let entries = {
            let log = ext.get_or_insert_default::<SupersededTransitionTraceLog>();
            log.take_entries()
        };

        if !entries.is_empty() {
            let state = ext.get_or_insert_default::<TransitionConflictState>();
            state.total_conflicts += entries.len() as u64;

            let mut seen = std::collections::BTreeSet::new();
            for entry in &entries {
                seen.insert(entry.preempted_by.clone());
                state.last_preempted_plugin = Some(entry.preempted_by.clone());
            }
            state.unique_preempted_plugins = seen.len() as u64;
        }

        // Rollback telemetry
        let events = {
            let log = ext.get_or_insert_default::<RollbackEventLog>();
            std::mem::take(&mut log.events)
        };

        let state = ext.get_or_insert_default::<RollbackTelemetryState>();

        for event in &events {
            if event.was_rollback {
                if event.budget_exceeded {
                    state.abandoned_count += 1;
                    state.abandoned_weight_total += event.frame_weight;
                } else {
                    state.restored_count += 1;
                }
            }

            let hook_name = event.hook_name.clone();
            if let Some(entry) = state
                .per_hook_stats
                .iter_mut()
                .find(|(name, _, _)| *name == hook_name)
            {
                if event.was_rollback && event.budget_exceeded {
                    entry.1 += 1;
                } else if event.was_rollback {
                    entry.2 += 1;
                }
            } else {
                let abandonments = if event.was_rollback && event.budget_exceeded {
                    1
                } else {
                    0
                };
                let restorations = if event.was_rollback && !event.budget_exceeded {
                    1
                } else {
                    0
                };
                state
                    .per_hook_stats
                    .push((hook_name, abandonments, restorations));
            }
        }

        Ok(())
    }
}
