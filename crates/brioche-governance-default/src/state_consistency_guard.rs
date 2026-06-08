//! StateConsistencyGuard — `ConsistencyVerifier` implementation (Book II §5.2).
//!
//! Verifies mechanical consistency after a transition. If the state is
//! `Predicting` or `ExecutingTools` without justification (empty stack),
//! forces a return to `Idle` with cleanup.
//!
//! Refs: I-Core-NoPanic, I-Gov-Decision-Required

use brioche_core::{
    AgentState, AgentStateTag, ConsistencyReport, ConsistencyVerifier, Effect, ErrorCode,
    PluginResult, Session,
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
    fn verify_consistency(&self, session: &Session) -> PluginResult<ConsistencyReport> {
        let tag = AgentStateTag::from(session.state());

        match tag {
            AgentStateTag::Predicting | AgentStateTag::ExecutingTools => {
                // An active state without a context stack is inconsistent:
                // there is no previous state to restore.
                if session.state_stack().is_empty() {
                    return Ok(ConsistencyReport {
                        suggested_state: Some(AgentState::Idle),
                        clear_stack: true,
                        effects: vec![
                            Effect::Error {
                                code: ErrorCode::StateInconsistency,
                                message: "inconsistent state: active without stack context".into(),
                            },
                            Effect::SaveSession,
                            Effect::SystemIdle,
                        ],
                    });
                }

                // ExecutingTools without active tools is also suspicious,
                // but the kernel already handles this case via `active_tools`.
                Ok(ConsistencyReport::ok())
            }
            _ => Ok(ConsistencyReport::ok()),
        }
    }
}
