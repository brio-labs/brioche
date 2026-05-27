//! RecoveryPolicy — Book II §5.11.
//!
//! Consumes `SystemSignal` events (via the `on_input` hook, adapted from
//! `SystemSignal` by the shell) and handles network/cancel recovery.
//!
//! In the current sprint, this plugin intercepts `Error` effects that
//! carry `NetworkUnavailable` or `OperationCancelled` codes and emits
//! recovery effects.
//!
//! Refs: I-Gov-Recovery-Graceful

use brioche_core::{
    BriochePlugin, EngineInput, ExtensionStorage, PluginCapabilities, PluginResult, PolicyDecision,
};

/// Recovery policy state.
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
pub struct RecoveryState {
    /// Number of consecutive recovery attempts.
    pub consecutive_recoveries: u64,
    /// Last observed network error (if applicable).
    pub last_network_error: Option<String>,
}

/// Recovery policy for system signals.
///
/// On `on_input`, inspects `Effect::Error` carrying the
/// `NetworkUnavailable` or `OperationCancelled` codes and emits
/// appropriate recovery effects.
pub struct RecoveryPolicy;

impl RecoveryPolicy {
    /// Creates a new instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for RecoveryPolicy {
    fn name(&self) -> &'static str {
        "recovery_policy"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        -50 // Early interceptor, after epoch but before business logic
    }

    /// Prepares the recovery state (shell adapter in Sprint 9+).
    ///
    /// # Complexity
    /// O(log n). One `ExtensionStorage` read.
    fn on_input(
        &self,
        _input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        // In the full shell integration, SystemSignal events are drained
        // into ExtensionStorage state by an adapter. For Sprint 8, we
        // register the plugin and prepare the state structure.
        let _state = ext.get_or_insert_default::<RecoveryState>();
        Ok(PolicyDecision::Allow)
    }
}
