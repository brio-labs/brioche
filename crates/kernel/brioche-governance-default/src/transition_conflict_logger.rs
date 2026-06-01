//! TransitionConflictLogger — Book II §5.13.
//!
//! Consumes `SupersededTransitionTraceLog` entries via the `after_prediction`
//! hook, archives them into `TransitionConflictState`, and clears the trace
//! log to prevent unbounded growth.
//!
//! Refs: I-Gov-OverrideTrace, I-Comp-Pure-Logic

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, SupersededTransitionTraceLog,
};

/// Archived transition conflict summary.
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
pub struct TransitionConflictLogger;

impl TransitionConflictLogger {
    /// Creates a new instance.
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
        100 // Late observer — let other after_prediction plugins run first
    }

    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        // Steal entries to avoid double mutable borrow of ext.
        let entries = {
            let log = ext.get_or_insert_default::<SupersededTransitionTraceLog>();
            std::mem::take(&mut log.entries)
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
