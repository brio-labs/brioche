//! Fundamental governance guards — Book II §5.
//!
//! Reference implementations of core governance traits:
//! - `EpochGuard`: `EpochInterceptor`
//! - `StateConsistencyGuard`: `ConsistencyVerifier`
//! - `SystemFailoverGuard`: `GovernanceFailoverHandler`
//!
//! Refs: I-Comp-Epoch-First, I-Core-NoPanic, I-Gov-Failover

use brioche_core::{
    AgentState, AgentStateTag, ConsistencyVerifier, Effect, EngineInput, EpochAction,
    EpochInterceptor, EpochState, ErrorCode, ErrorDetail, ExtensionStorage,
    GovernanceFailoverHandler, PluginResult, Session,
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
    fn verify_consistency(&self, session: &mut Session) -> PluginResult<Option<Vec<Effect>>> {
        let tag = AgentStateTag::from(&session.state);

        match tag {
            AgentStateTag::Predicting | AgentStateTag::ExecutingTools => {
                if session.state_stack.is_empty() {
                    let effects = vec![
                        Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            detail: ErrorDetail::StateInconsistent {
                                source: "active without stack context".into(),
                            },
                        },
                        Effect::SaveSession,
                        Effect::SystemIdle,
                    ];

                    session.state = AgentState::Idle;
                    session.state_stack.clear();
                    session.active_tools.clear();

                    return Ok(Some(effects));
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
