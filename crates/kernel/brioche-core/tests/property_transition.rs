//! Property tests for `transition()` — Sprint 18.
//!
//! Invariants verified:
//! - `transition()` never panics for arbitrary valid inputs.
//! - Identical inputs produce identical `Vec<Effect>` outputs (bit-for-bit determinism).
//! - Invalid state transitions produce `BriocheError`, not panics.
//!
//! Refs: I-Core-NoPanic, I-Core-Pure

use brioche_core::{
    AgentState, BriocheEngineBuilder, DecisionAggregator, Effect, EngineInput, ExecutionPath,
    ExtensionStorage, PluginResult, PolicyDecision, Session, StreamEvent, SubRoutineLifecycleGuard,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Mock governance traits
// ---------------------------------------------------------------------------

struct MockDecisionAggregator;
impl DecisionAggregator for MockDecisionAggregator {
    fn aggregate_decisions(
        &self,
        _decisions: Vec<PolicyDecision>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

struct MockSubRoutineLifecycleGuard;
impl SubRoutineLifecycleGuard for MockSubRoutineLifecycleGuard {
    fn on_exit(
        &self,
        _handle: brioche_core::SubRoutineHandle,
        _parent: &mut Session,
        _registry: &mut brioche_core::SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        Ok(vec![])
    }
}

fn build_engine() -> brioche_core::BriocheEngine {
    match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(_) => std::process::abort(),
    }
}

// ---------------------------------------------------------------------------
// Proptest strategies
// ---------------------------------------------------------------------------

fn engine_input_strategy() -> impl Strategy<Value = EngineInput> {
    prop_oneof![
        "[a-zA-Z0-9 ]{0,32}".prop_map(EngineInput::UserMessage),
        prop_oneof![
            Just(StreamEvent::Done),
            Just(StreamEvent::Pass),
            ("[a-z0-9]{1,8}", "[a-z0-9]{1,8}").prop_map(|(id, name)| StreamEvent::ToolCallStart {
                path: ExecutionPath::default(),
                id,
                name,
            }),
        ]
        .prop_map(EngineInput::LlmStream),
    ]
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn prop_transition_never_panics(input in engine_input_strategy()) {
        let mut engine = build_engine();
        let mut session = Session::new("prop");
        // Ensure session is in a state that can receive the input.
        if matches!(input, EngineInput::LlmStream(_)) {
            let _ = session.push_state(AgentState::Predicting { generation_id: 1 });
        }

        let _effects = engine.transition(&mut session, &input);
        // If we reach this point, transition() did not panic.
    }

    #[test]
    fn prop_bit_for_bit_determinism(input in engine_input_strategy()) {
        let mut engine_a = build_engine();
        let mut engine_b = build_engine();
        let mut session_a = Session::new("det");
        let mut session_b = Session::new("det");

        if matches!(input, EngineInput::LlmStream(_)) {
            let _ = session_a.push_state(AgentState::Predicting { generation_id: 1 });
            let _ = session_b.push_state(AgentState::Predicting { generation_id: 1 });
        }

        let effects_a = engine_a.transition(&mut session_a, &input);
        let effects_b = engine_b.transition(&mut session_b, &input);

        prop_assert_eq!(effects_a, effects_b);
        prop_assert_eq!(session_a.state, session_b.state);
    }

    #[test]
    fn prop_invalid_stack_op_produces_error(state in prop_oneof![
        Just(AgentState::Idle),
        Just(AgentState::Failure),
    ]) {
        // pop_state on empty stack should produce BriocheError, not panic.
        let mut session = Session::new("prop");
        session.state = state;
        let result = session.pop_state();
        prop_assert!(result.is_err());
    }
}
