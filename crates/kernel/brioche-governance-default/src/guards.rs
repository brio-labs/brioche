//! Fundamental governance guards — Book II §5.
//!
//! Reference implementations of core governance traits:
//! - `EpochGuard`: `EpochInterceptor`
//! - `StateConsistencyGuard`: `ConsistencyVerifier`
//! - `SystemFailoverGuard`: `GovernanceFailoverHandler`
//!
//! Refs: I-Comp-Epoch-First, I-Core-NoPanic, I-Gov-Failover

use brioche_core::types::InconsistencySource;
use brioche_core::{
    AgentStateTag, ConsistencyVerifier, Effect, EngineInput, EpochAction, EpochInterceptor,
    EpochState, ErrorCode, ErrorDetail, ExtensionStorage, GovernanceFailoverHandler, PluginResult,
    PolicyDecision, Session,
};

// ---------------------------------------------------------------------------
// EpochGuard
// ---------------------------------------------------------------------------

/// Temporal barrier manager by epochs.
///
/// `intercept_epoch` compares the `generation_id` carried by an
/// `EngineInput::ToolCallsResult` with `EpochState.current_generation`.
/// In case of divergence, the input is silently rejected.
///
/// # Invariants
/// - Refs: I-Gov-Epoch-Reject — rejects asynchronous responses from past epochs.
/// - Refs: I-Comp-Epoch-First — always evaluated first in the cycle.
pub struct EpochGuard;

impl EpochInterceptor for EpochGuard {
    type EngineInput = EngineInput;
    type EpochAction = EpochAction;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn intercept_epoch(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<EpochAction> {
        let epoch_state = ext.get_or_insert_default::<EpochState>();

        match input {
            EngineInput::ToolCallsResult { generation_id, .. } => {
                if *generation_id != epoch_state.current_generation {
                    return Ok(EpochAction::Block {
                        reason: format!(
                            "epoch mismatch: expected {}, got {}",
                            epoch_state.current_generation, generation_id
                        ),
                    });
                }
                Ok(EpochAction::Proceed)
            }
            _ => Ok(EpochAction::Proceed),
        }
    }
}

// ---------------------------------------------------------------------------
// StateConsistencyGuard
// ---------------------------------------------------------------------------

/// Mechanical state consistency verifier.
///
/// This guard is optional but recommended in production. Without injection,
/// the kernel does not verify consistency — which may leave the automaton
/// in an inconsistent state after a malformed `OverrideTransition`.
///
/// Refs: I-Core-NoPanic, I-Gov-Decision-Required
pub struct StateConsistencyGuard;

impl StateConsistencyGuard {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for StateConsistencyGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsistencyVerifier for StateConsistencyGuard {
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;
    type Session = Session;

    fn verify_consistency(&self, session: &Session) -> PluginResult<Option<PolicyDecision>> {
        let tag = AgentStateTag::from(&session.state);

        match tag {
            AgentStateTag::Predicting | AgentStateTag::ExecutingTools => {
                if session.state_stack.is_empty() {
                    let effects = vec![
                        Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            detail: ErrorDetail::StateInconsistent {
                                source: InconsistencySource::Kernel {
                                    module: "guards::consistency_verifier".to_string(),
                                },
                            },
                        },
                        Effect::SaveSession,
                        Effect::SystemIdle,
                    ];

                    return Ok(Some(PolicyDecision::OverrideTransition(effects)));
                }

                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// SystemFailoverGuard
// ---------------------------------------------------------------------------

/// System failover guard.
///
/// Intercepts `Effect::PluginFault` emanating from fundamental plugins
/// and replaces the effect sequence with a safe terminal state.
///
/// Refs: I-Gov-TraitAtomic
/// Refs: I-Gov-Failover
pub struct SystemFailoverGuard;

impl SystemFailoverGuard {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemFailoverGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl GovernanceFailoverHandler for SystemFailoverGuard {
    type Effect = Effect;
    type PluginError = brioche_core::PluginError;
    type Session = Session;

    fn handle_failure(
        &self,
        _session: &mut Session,
        fault: &Effect,
    ) -> PluginResult<Option<Vec<Effect>>> {
        let plugin_name = match fault {
            Effect::PluginFault { plugin_name, .. } => plugin_name.clone(),
            _ => return Ok(None),
        };

        Ok(Some(vec![
            Effect::ForwardToUi(brioche_core::UiWidget::CriticalError {
                component: plugin_name.0,
                detail: Some("governance component failed; system degraded".into()),
            }),
            Effect::SaveSession,
            Effect::SystemIdle,
        ]))
    }
}

#[cfg(test)]
mod tests {
    use brioche_core::{AgentState, BriocheError};

    use super::*;

    #[test]
    fn consistency_guard_returns_override_transition_on_empty_stack() -> Result<(), BriocheError> {
        let guard = StateConsistencyGuard::new();
        let mut session = Session::new("test");
        // Simulate an inconsistent OverrideTransition that set Predicting
        // without preserving a previous state on the stack.
        session.state = AgentState::Predicting { generation_id: 1 };

        let decision = guard
            .verify_consistency(&session)
            .map_err(|e| BriocheError::InvalidStateTransition(e.to_string()))?;

        assert!(
            matches!(decision, Some(PolicyDecision::OverrideTransition(_))),
            "expected OverrideTransition for inconsistent state"
        );
        // The guard must not mutate the session; recovery is applied by the kernel.
        assert!(matches!(
            session.state,
            AgentState::Predicting { generation_id: 1 }
        ));
        assert!(session.state_stack.is_empty());
        Ok(())
    }

    #[test]
    fn consistency_guard_allows_consistent_state() -> Result<(), BriocheError> {
        let guard = StateConsistencyGuard::new();
        let mut session = Session::new("test");
        session.push_state(AgentState::Predicting { generation_id: 1 })?;
        session.push_state(AgentState::ExecutingTools { generation_id: 1 })?;

        let decision = guard
            .verify_consistency(&session)
            .map_err(|e| BriocheError::InvalidStateTransition(e.to_string()))?;

        assert_eq!(decision, None);
        Ok(())
    }
}
