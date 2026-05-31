//! RecoveryPolicy — Book II §5.11.
//!
//! Circuit-breaker style recovery guard. Inspects `SessionSnapshot` in
//! `ExtensionStorage` to detect consecutive `Failure` states. If the
//! failure count exceeds `max_consecutive_recoveries`, subsequent inputs
//! are blocked with `PolicyDecision::Block(Effect::SystemIdle)` until
//! the session returns to a healthy state.
//!
//! Refs: I-Gov-Recovery-Graceful, I-Comp-Pure-Logic

use brioche_core::{
    AgentStateTag, BriochePlugin, EngineInput, ExtensionStorage, PluginCapabilities, PluginResult,
    PolicyDecision, SessionSnapshot,
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
pub struct RecoveryPolicy {
    max_consecutive_recoveries: u64,
}

impl RecoveryPolicy {
    /// Creates a policy with the default threshold (3).
    pub fn new() -> Self {
        Self {
            max_consecutive_recoveries: DEFAULT_MAX_RECOVERIES,
        }
    }

    /// Creates a policy with a custom threshold (0 = never block).
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
        -50 // Early interceptor, after epoch but before business logic
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
