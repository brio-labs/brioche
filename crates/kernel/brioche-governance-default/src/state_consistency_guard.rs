//! StateConsistencyGuard — `ConsistencyVerifier` implementation (Book II §5.2).
//!
//! Verifies mechanical consistency after a transition. If the state is
//! `Predicting` or `ExecutingTools` without justification (empty stack),
//! forces a return to `Idle` with cleanup.
//!
//! Refs: I-Core-NoPanic, I-Gov-Decision-Required

use brioche_core::{
    AgentState, AgentStateTag, ConsistencyVerifier, Effect, ErrorCode, ErrorDetail, PluginResult,
    Session,
};

/// Mechanical state consistency verifier.
///
/// This guard is optional but recommended in production. Without injection,
/// the kernel does not verify consistency — which may leave the automaton
/// in an inconsistent state after a malformed `OverrideTransition`.
pub struct StateConsistencyGuard;

impl StateConsistencyGuard {
    /// Creates a new instance.
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
                // An active state without a context stack is inconsistent:
                // there is no previous state to restore.
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

                    // Mechanical forcing to Idle with cleanup.
                    session.state = AgentState::Idle;
                    session.state_stack.clear();
                    session.active_tools.clear();

                    return Ok(Some(effects));
                }

                // ExecutingTools without active tools is also suspicious,
                // but the kernel already handles this case via `active_tools`.
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
