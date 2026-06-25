//! Property tests for `transition()` — Sprint 18.
//!
//! Invariants verified:
//! - `transition()` never panics for arbitrary valid inputs.
//! - Identical inputs produce identical `Vec<Effect>` outputs (bit-for-bit determinism).
//! - Invalid state transitions produce `BriocheError`, not panics.
//!
//! Refs: I-Core-NoPanic, I-Core-Pure

use brioche_core::{
    AgentState, BriocheEngineBuilder, BriochePlugin, ChatMessage, DecisionAggregator, Effect,
    EngineInput, ExecutionPath, ExtensionStorage, PluginCapabilities, PluginResult, PolicyDecision,
    Session, StreamAction, StreamEvent, SubRoutineLifecycleGuard, ToolCallDescriptor,
    ToolResultDTO,
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
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
}

/// Engine with two pure plugins registered in a specific order.
fn build_engine_with_plugins(a_first: bool) -> brioche_core::BriocheEngine {
    let mut builder = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard));

    if a_first {
        builder = builder
            .with_plugin(Box::new(PurePluginA))
            .with_plugin(Box::new(PurePluginB));
    } else {
        builder = builder
            .with_plugin(Box::new(PurePluginB))
            .with_plugin(Box::new(PurePluginA));
    }
    builder.build()
}

// ---------------------------------------------------------------------------
// Pure plugins for order-independence tests
// ---------------------------------------------------------------------------

/// Pure plugin A — always Allow, no side effects.
struct PurePluginA;

impl BriochePlugin for PurePluginA {
    fn name(&self) -> &'static str {
        "pure_a"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
            | PluginCapabilities::BEFORE_PREDICTION
            | PluginCapabilities::ON_STREAM_EVENT
            | PluginCapabilities::AFTER_PREDICTION
            | PluginCapabilities::ON_TOOL_CALLS
            | PluginCapabilities::ON_TOOL_RESULT
    }

    fn priority(&self) -> i16 {
        0
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    fn on_stream_event(
        &self,
        _event: &StreamEvent,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        Ok(StreamAction::Pass)
    }

    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }

    fn on_tool_calls(
        &self,
        _calls: &mut Vec<ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }

    fn on_tool_result(
        &self,
        _results: &mut Vec<ToolResultDTO>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }
}

/// Pure plugin B — always Allow, no side effects.
struct PurePluginB;

impl BriochePlugin for PurePluginB {
    fn name(&self) -> &'static str {
        "pure_b"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
            | PluginCapabilities::BEFORE_PREDICTION
            | PluginCapabilities::ON_STREAM_EVENT
            | PluginCapabilities::AFTER_PREDICTION
            | PluginCapabilities::ON_TOOL_CALLS
            | PluginCapabilities::ON_TOOL_RESULT
    }

    fn priority(&self) -> i16 {
        0
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    fn on_stream_event(
        &self,
        _event: &StreamEvent,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        Ok(StreamAction::Pass)
    }

    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }

    fn on_tool_calls(
        &self,
        _calls: &mut Vec<ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }

    fn on_tool_result(
        &self,
        _results: &mut Vec<ToolResultDTO>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
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
            Just(StreamEvent::Error {
                message: "test error".into(),
            }),
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

    #[test]
    fn prop_generation_id_monotonicity(count in 1usize..10) {
        // Repeated UserMessage transitions must produce strictly increasing
        // generation IDs. This is a fundamental determinism invariant.
        let mut engine = build_engine();
        let mut session = Session::new("prop");
        let mut prev_gen: Option<u64> = None;

        for i in 0..count {
            let effects = engine.transition(
                &mut session,
                &EngineInput::UserMessage(format!("msg-{}", i)),
            );

            // Should transition to Predicting with a generation ID.
            let current_gen = match session.state {
                AgentState::Predicting { generation_id } => generation_id,
                _ => {
                    prop_assert!(false, "expected Predicting state after UserMessage");
                    return Ok(());
                }
            };

            if let Some(prev) = prev_gen {
                prop_assert!(
                    current_gen > prev,
                    "generation_id should be strictly increasing: {} > {}",
                    current_gen,
                    prev
                );
            }
            prev_gen = Some(current_gen);

            // Pop back to Idle so next transition works.
            let pop_result = session.pop_state();
            prop_assert!(pop_result.is_ok(), "pop_state should succeed");

            // Effects should contain CallLlmNetwork and SaveSession.
            prop_assert!(
                effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)),
                "transition {} should emit CallLlmNetwork",
                i
            );
            prop_assert!(
                effects.iter().any(|e| matches!(e, Effect::SaveSession)),
                "transition {} should emit SaveSession",
                i
            );
        }
    }

    #[test]
    fn prop_pure_plugin_order_independence(input in engine_input_strategy()) {
        // Two pure plugins with identical priority should produce identical
        // effects regardless of registration order. This verifies that the
        // routing table's total order is deterministic and that pure plugins
        // do not interfere with each other.
        let mut engine_a = build_engine_with_plugins(true);  // A first
        let mut engine_b = build_engine_with_plugins(false); // B first
        let mut session_a = Session::new("order");
        let mut session_b = Session::new("order");

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
    fn prop_effects_imply_state(input in engine_input_strategy()) {
        // Forward consistency: specific effects imply specific states.
        // - CallLlmNetwork  → state must be Predicting
        // - SystemIdle      → state must be Idle
        // - ExecuteTools(_) → state must be ExecutingTools
        let mut engine = build_engine();
        let mut session = Session::new("prop");

        if matches!(input, EngineInput::LlmStream(_)) {
            let _ = session.push_state(AgentState::Predicting { generation_id: 1 });
        }

        let effects = engine.transition(&mut session, &input);

        if effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)) {
            prop_assert!(
                matches!(session.state, AgentState::Predicting { .. }),
                "CallLlmNetwork effect implies Predicting state, got {:?}",
                session.state
            );
        }
        if effects.iter().any(|e| matches!(e, Effect::SystemIdle)) {
            prop_assert!(
                matches!(session.state, AgentState::Idle),
                "SystemIdle effect implies Idle state, got {:?}",
                session.state
            );
        }
        if effects.iter().any(|e| matches!(e, Effect::ExecuteTools(_))) {
            prop_assert!(
                matches!(session.state, AgentState::ExecutingTools { .. }),
                "ExecuteTools effect implies ExecutingTools state, got {:?}",
                session.state
            );
        }
    }
}
