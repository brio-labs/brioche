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
use std::collections::{BTreeMap, BTreeSet};

/// State tracking quarantined plugins.
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with number of quarantined plugins
/// and fault counts (typically 003c 20). One `BTreeSet` + one `BTreeMap`.
///
/// Stored in `ExtensionStorage`. Uses `BTreeSet` and `BTreeMap` for
/// deterministic iteration and O(log n) lookup.
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
    pub quarantined: BTreeSet<String>,
    /// Count of faults observed per plugin (for escalation policies).
    /// `BTreeMap` guarantees deterministic iteration and O(log n) lookup.
    pub fault_counts: BTreeMap<PluginFaultKey, u64>,
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

            // Increment fault count via BTreeMap for O(log n) lookup.
            let key = PluginFaultKey {
                plugin_name: plugin_name.clone(),
                error_kind: format!("{:?}", std::mem::discriminant(error)),
            };
            *state.fault_counts.entry(key).or_insert(0) += 1;

            return Ok(PolicyDecision::RequestEffect(Effect::RebuildRoutes));
        }

        Ok(PolicyDecision::Allow)
    }
}
