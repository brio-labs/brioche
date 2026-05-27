//! TransitionConflictLogger — Book II §5.13.
//!
//! Consumes `SupersededTransitionTraceLog` entries via the `after_prediction`
//! hook for asynchronous archiving. This plugin does not modify state; it
//! passively observes and could emit telemetry effects in a full shell.
//!
//! Refs: I-Gov-OverrideTrace

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, SupersededTransitionTraceLog,
};

/// Logger de conflits de transition.
///
/// Sur `after_prediction`, inspecte le `SupersededTransitionTraceLog`
/// pour détecter les `OverrideTransition` ayant été préemptés.
pub struct TransitionConflictLogger;

impl TransitionConflictLogger {
    /// Crée une nouvelle instance.
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
        let log = ext.get_or_insert_default::<SupersededTransitionTraceLog>();

        // In a full implementation, this would emit telemetry effects
        // or archive to cold storage. For Sprint 8, we simply ensure
        // the log is accessible and non-empty if conflicts occurred.
        let _count = log.entries.len();

        Ok(())
    }
}
