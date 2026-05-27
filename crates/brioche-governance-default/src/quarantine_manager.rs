//! QuarantineManager — Book II §5.10.
//!
//! Consumes `GovernanceNotification::PluginFaulted` (via the `on_error`
//! hook, which the kernel calls when it intercepts a plugin error) and
//! decides whether to quarantine the plugin, emitting `RebuildRoutes`.
//!
//! Refs: I-Gov-Quarantine-Isolate, I-Comp-Override-Rebuild

use brioche_core::{
    BriochePlugin, Effect, ExtensionStorage, PluginCapabilities, PluginError, PluginResult,
    PolicyDecision,
};
use std::collections::BTreeSet;

/// State tracking quarantined plugins.
///
/// Stored in `ExtensionStorage`. Uses `BTreeSet` for deterministic iteration.
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
pub struct QuarantineState {
    /// Set of quarantined plugin names.
    #[brioche(deterministic_order)]
    pub quarantined: BTreeSet<String>,
    /// Count of faults observed per plugin (for escalation policies).
    #[brioche(deterministic_order)]
    pub fault_counts: Vec<(PluginFaultKey, u64)>,
}

/// Deterministic key for fault counting.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PluginFaultKey {
    pub plugin_name: String,
    pub error_kind: String,
}

/// Manager de quarantaine des plugins.
///
/// On `on_error`, if the error is `Fatal`, the plugin is added to
/// `QuarantineState` and a `RebuildRoutes` is requested.
pub struct QuarantineManager;

impl QuarantineManager {
    /// Creates a new instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for QuarantineManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for QuarantineManager {
    fn name(&self) -> &'static str {
        "quarantine_manager"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_ERROR
    }

    fn priority(&self) -> i16 {
        -100 // Very early on_error handler
    }

    fn on_error(
        &self,
        error: &PluginError,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let state = ext.get_or_insert_default::<QuarantineState>();

        let plugin_name = match error {
            PluginError::Soft { plugin_name, .. } => plugin_name.clone(),
            PluginError::Fatal { plugin_name, .. } => plugin_name.clone(),
        };

        // Only quarantine on fatal errors.
        if matches!(error, PluginError::Fatal { .. }) {
            state.quarantined.insert(plugin_name.clone());

            // Increment fault count.
            let key = PluginFaultKey {
                plugin_name: plugin_name.clone(),
                error_kind: format!("{:?}", std::mem::discriminant(error)),
            };
            let mut found = false;
            for (k, count) in &mut state.fault_counts {
                if *k == key {
                    *count += 1;
                    found = true;
                    break;
                }
            }
            if !found {
                state.fault_counts.push((key, 1));
            }

            return Ok(PolicyDecision::RequestEffect(Effect::RebuildRoutes));
        }

        Ok(PolicyDecision::Allow)
    }
}
