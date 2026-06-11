//! Telemetry observers — Book II §5.
//!
//! Reference implementations for event tracking and logging:
//! - `ToolCallDetector`: counts tool call stream events
//! - `TransitionConflictLogger`: archives transition conflict traces
//!
//! Refs: I-Core-ActiveToolCall, I-Gov-OverrideTrace

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, StreamAction, StreamEvent,
    SupersededTransitionTraceLog,
};

// ---------------------------------------------------------------------------
// ToolCallDetector
// ---------------------------------------------------------------------------

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

/// Tool call detector in the stream.
///
/// On `on_stream_event`, increments counters during
/// `ToolCallStart` and `ToolCallDone` events.
///
/// Refs: I-Core-ActiveToolCall
pub struct ToolCallDetector;

impl ToolCallDetector {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolCallDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for ToolCallDetector {
    fn name(&self) -> &'static str {
        "tool_call_detector"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_STREAM_EVENT
    }

    fn priority(&self) -> i16 {
        10
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

// ---------------------------------------------------------------------------
// TransitionConflictLogger
// ---------------------------------------------------------------------------

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

/// Transition conflict logger.
///
/// On `after_prediction`, inspects `SupersededTransitionTraceLog` to
/// detect `OverrideTransition`s that have been preempted, archives the
/// summary into `TransitionConflictState`, and clears the trace log.
///
/// Refs: I-Gov-OverrideTrace
pub struct TransitionConflictLogger;

impl TransitionConflictLogger {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for TransitionConflictLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for TransitionConflictLogger {
    fn name(&self) -> &'static str {
        "transition_conflict_logger"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::AFTER_PREDICTION
    }

    fn priority(&self) -> i16 {
        100
    }

    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        let entries = {
            let log = ext.get_or_insert_default::<SupersededTransitionTraceLog>();
            log.take_entries()
        };

        if entries.is_empty() {
            return Ok(());
        }

        let state = ext.get_or_insert_default::<TransitionConflictState>();
        state.total_conflicts += entries.len() as u64;

        let mut seen = std::collections::BTreeSet::new();
        for entry in &entries {
            seen.insert(entry.preempted_by.clone());
            state.last_preempted_plugin = Some(entry.preempted_by.clone());
        }
        state.unique_preempted_plugins = seen.len() as u64;

        Ok(())
    }
}
