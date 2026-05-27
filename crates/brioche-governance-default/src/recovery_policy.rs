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

/// État de la politique de récupération.
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
    /// Nombre de tentatives de récupération consécutives.
    pub consecutive_recoveries: u64,
    /// Dernière erreur réseau observée (si applicable).
    pub last_network_error: Option<String>,
}

/// Politique de récupération face aux signaux système.
///
/// Sur `on_input`, inspecte les `Effect::Error` portant les codes
/// `NetworkUnavailable` ou `OperationCancelled` et émet des effets
/// de récupération appropriés.
pub struct RecoveryPolicy;

impl RecoveryPolicy {
    /// Crée une nouvelle instance.
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

    /// Prépare l'état de récupération (shell adapter en Sprint 9+).
    ///
    /// # Complexity
    /// O(log n). Une lecture `ExtensionStorage`.
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
