//! SubRoutineTimeoutPolicy — Book II §5.14.
//!
//! Consumes `SystemSignal::Tick` (via shell adapter) and limits
//! sub-routine lifetime. In the current architecture, this plugin
//! registers state and would be triggered by a shell-side adapter
//! that drains tick signals into the engine.
//!
//! Refs: I-Gov-SubRoutineLifecycle-Guard

use brioche_core::{
    BriochePlugin, EngineInput, ExtensionStorage, PluginCapabilities, PluginResult, PolicyDecision,
};
use std::collections::BTreeMap;

/// État des timers de sous-routines.
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
#[brioche(critical_state)]
pub struct SubRoutineTimerState {
    /// Map handle -> (timestamp_start_ms, timeout_limit_ms).
    pub timers: BTreeMap<brioche_core::SubRoutineHandle, (u64, u64)>,
}

/// Politique de timeout pour les sous-routines.
///
/// Sur `on_input`, vérifie si une sous-routine a dépassé son timeout.
/// Si oui, émet un `OverrideTransition` vers `Idle`.
pub struct SubRoutineTimeoutPolicy {
    #[allow(dead_code)]
    default_timeout_ms: u64,
}

impl SubRoutineTimeoutPolicy {
    /// Crée une politique avec un timeout par défaut.
    pub fn with_default_timeout(default_timeout_ms: u64) -> Self {
        Self { default_timeout_ms }
    }
}

impl Default for SubRoutineTimeoutPolicy {
    fn default() -> Self {
        Self::with_default_timeout(300000)
    }
}

impl BriochePlugin for SubRoutineTimeoutPolicy {
    fn name(&self) -> &'static str {
        "subroutine_timeout_policy"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        -30 // After epoch, recovery, depth
    }

    /// Prépare l'état des timers (drainage shell en Sprint 9+).
    ///
    /// # Complexity
    /// O(log n). Une lecture `ExtensionStorage`.
    fn on_input(
        &self,
        _input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let _state = ext.get_or_insert_default::<SubRoutineTimerState>();

        // Full tick-based timeout logic requires shell-side SystemSignal::Tick
        // drainage. For Sprint 8, we register the state structure.
        // The shell adapter will populate timers and check elapsed time.
        Ok(PolicyDecision::Allow)
    }
}
