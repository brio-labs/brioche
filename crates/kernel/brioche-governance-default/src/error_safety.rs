//! Error handling & recovery plugins.
//!
//! This module groups plugins that react to faults and failures:
//! - `QuarantineManager`: isolates plugins that fault fatally.
//! - `RecoveryPolicy`: circuit-breaker style recovery guard.
//!
//! Refs: I-Gov-Quarantine-Isolate, I-Gov-Recovery-Graceful

use std::collections::{BTreeMap, BTreeSet};

use brioche_core::{
    AgentStateTag, BriochePlugin, Effect, EngineInput, ExtensionStorage, PluginCapabilities,
    PluginError, PluginResult, PolicyDecision, SessionSnapshot,
};

use crate::Priority;

// ---------------------------------------------------------------------------
// QuarantineManager
// ---------------------------------------------------------------------------

/// State tracking quarantined plugins.
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with number of quarantined plugins
/// and fault counts (typically < 20). One `BTreeSet` + one `BTreeMap`.
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
///
/// Refs: I-Gov-Quarantine-Isolate
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct PluginFaultKey {
    /// Name of the plugin that faulted.
    pub plugin_name: String,
    /// Discriminant string of the error kind.
    pub error_kind: String,
}

/// Plugin quarantine manager.
///
/// On `on_error`, if the error is `Fatal`, the plugin is added to
/// `QuarantineState` and a `RebuildRoutes` is requested.
///
/// Refs: I-Gov-Quarantine-Isolate, I-Comp-Override-Rebuild
pub struct QuarantineManager;

impl QuarantineManager {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
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
        Priority::QUARANTINE // Very early on_error handler
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
            _ => String::new(),
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

// ---------------------------------------------------------------------------
// RecoveryPolicy
// ---------------------------------------------------------------------------

/// Recovery policy state.
///
/// ## Snapshot strategy
/// COW: full clone (~40 bytes). Three scalars + one optional short String.
///
/// Refs: I-Gov-Recovery-Graceful
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
    /// Last observed error message (if applicable).
    pub last_error: Option<String>,
    /// Total number of inputs blocked by this policy.
    pub inputs_blocked: u64,
}

/// Maximum consecutive failures before the circuit opens.
const DEFAULT_MAX_RECOVERIES: u64 = 3;

/// Recovery policy — circuit breaker for cascading failures.
///
/// Monitors `SessionSnapshot` to detect when the session is stuck in
/// `Failure`. After `max_consecutive_recoveries` consecutive failures,
/// blocks all further input until the session recovers.
///
/// Refs: I-Gov-TraitAtomic
/// Refs: I-Gov-Recovery-Graceful
pub struct RecoveryPolicy {
    max_consecutive_recoveries: u64,
}

impl RecoveryPolicy {
    /// Creates a policy with the default threshold (3).
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self {
            max_consecutive_recoveries: DEFAULT_MAX_RECOVERIES,
        }
    }

    /// Creates a policy with a custom threshold (0 = never block).
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_max_recoveries(max_consecutive_recoveries: u64) -> Self {
        Self {
            max_consecutive_recoveries,
        }
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
        Priority::RECOVERY // Early interceptor, after epoch but before business logic
    }

    /// Prepares the recovery state (shell adapter in Sprint 9+).
    ///
    /// # Complexity
    /// O(log n). One `ExtensionStorage` read.
    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
        let is_failure = snapshot.current_state == AgentStateTag::Failure;
        let was_failure = {
            let state = ext.get_or_insert_default::<RecoveryState>();
            state.consecutive_recoveries >= self.max_consecutive_recoveries
        };

        if is_failure {
            let state = ext.get_or_insert_default::<RecoveryState>();
            state.consecutive_recoveries += 1;
            state.last_error = Some(format!("{:?}", input));

            if state.consecutive_recoveries >= self.max_consecutive_recoveries {
                state.inputs_blocked += 1;
                return Ok(PolicyDecision::Block {
                    reason: "recovery: max consecutive failures exceeded".into(),
                });
            }
        } else if !was_failure {
            // Healthy state — reset the counter.
            let state = ext.get_or_insert_default::<RecoveryState>();
            if state.consecutive_recoveries > 0 {
                state.consecutive_recoveries = 0;
                state.last_error = None;
            }
        }

        Ok(PolicyDecision::Allow)
    }
}

#[cfg(test)]
mod tests {
    use brioche_core::{
        AgentStateTag, BriochePlugin, Effect, EngineInput, ExtensionStorage, PluginError,
        PolicyDecision, SessionSnapshot,
    };

    use super::{QuarantineManager, QuarantineState, RecoveryPolicy, RecoveryState};

    #[test]
    fn quarantine_manager_allows_soft_error() {
        let manager = QuarantineManager::new();
        let mut ext = ExtensionStorage::new();

        let error = PluginError::Soft {
            plugin_name: "soft_plugin".into(),
            message: "minor".into(),
        };

        let decision = manager.on_error(&error, &mut ext);
        assert!(matches!(decision, Ok(PolicyDecision::Allow)));

        let state = ext.get_or_insert_default::<QuarantineState>();
        assert!(state.quarantined.is_empty());
    }

    #[test]
    fn quarantine_manager_quarantines_on_fatal() {
        let manager = QuarantineManager::new();
        let mut ext = ExtensionStorage::new();

        let error = PluginError::Fatal {
            plugin_name: "bad_plugin".into(),
            message: "simulated fatal".into(),
        };

        let decision = manager.on_error(&error, &mut ext);
        assert!(matches!(
            decision,
            Ok(PolicyDecision::RequestEffect(Effect::RebuildRoutes))
        ));

        let state = ext.get_or_insert_default::<QuarantineState>();
        assert!(state.quarantined.contains("bad_plugin"));
    }

    #[test]
    fn quarantine_manager_increments_fault_counts() {
        let manager = QuarantineManager::new();
        let mut ext = ExtensionStorage::new();

        let error = PluginError::Fatal {
            plugin_name: "bad_plugin".into(),
            message: "first".into(),
        };

        let _ = manager.on_error(&error, &mut ext);
        let _ = manager.on_error(&error, &mut ext);

        let state = ext.get_or_insert_default::<QuarantineState>();
        assert_eq!(state.quarantined.len(), 1);
        assert_eq!(state.fault_counts.len(), 1);
        let count = match state.fault_counts.values().next() {
            Some(&v) => v,
            None => 0,
        };
        assert_eq!(count, 2);
    }

    #[test]
    fn recovery_policy_allows_healthy_state() {
        let policy = RecoveryPolicy::new();
        let mut ext = ExtensionStorage::new();
        ext.insert(SessionSnapshot {
            current_state: AgentStateTag::Idle,
            state_stack_depth: 0,
        });

        let decision = policy.on_input(&EngineInput::UserMessage("hi".into()), &mut ext);
        assert!(matches!(decision, Ok(PolicyDecision::Allow)));
    }

    #[test]
    fn recovery_policy_increments_on_failure() {
        let policy = RecoveryPolicy::with_max_recoveries(3);
        let mut ext = ExtensionStorage::new();
        ext.insert(SessionSnapshot {
            current_state: AgentStateTag::Failure,
            state_stack_depth: 0,
        });

        let _ = policy.on_input(&EngineInput::UserMessage("try".into()), &mut ext);
        let state = ext.get_or_insert_default::<RecoveryState>();
        assert_eq!(state.consecutive_recoveries, 1);
        assert!(state.last_error.is_some());
    }

    #[test]
    fn recovery_policy_blocks_after_max_recoveries() {
        let policy = RecoveryPolicy::with_max_recoveries(2);
        let mut ext = ExtensionStorage::new();
        ext.insert(SessionSnapshot {
            current_state: AgentStateTag::Failure,
            state_stack_depth: 0,
        });

        let first = policy.on_input(&EngineInput::UserMessage("a".into()), &mut ext);
        assert!(matches!(first, Ok(PolicyDecision::Allow)));

        let second = policy.on_input(&EngineInput::UserMessage("b".into()), &mut ext);
        assert!(matches!(second, Ok(PolicyDecision::Block { .. })));

        let state = ext.get_or_insert_default::<RecoveryState>();
        assert_eq!(state.inputs_blocked, 1);
    }

    #[test]
    fn recovery_policy_resets_after_recovery() {
        let policy = RecoveryPolicy::with_max_recoveries(3);
        let mut ext = ExtensionStorage::new();
        ext.insert(SessionSnapshot {
            current_state: AgentStateTag::Failure,
            state_stack_depth: 0,
        });

        let _ = policy.on_input(&EngineInput::UserMessage("fail".into()), &mut ext);

        ext.insert(SessionSnapshot {
            current_state: AgentStateTag::Idle,
            state_stack_depth: 0,
        });
        let _ = policy.on_input(&EngineInput::UserMessage("ok".into()), &mut ext);

        let state = ext.get_or_insert_default::<RecoveryState>();
        assert_eq!(state.consecutive_recoveries, 0);
        assert!(state.last_error.is_none());
    }
}
