//! Property tests for `transition()` — Sprint 18.
//!
//! Invariants verified:
//! - `transition()` never panics for arbitrary valid inputs.
//! - Identical inputs produce identical `Vec<Effect>` outputs (bit-for-bit determinism).
//! - Invalid state transitions produce `BriocheError`, not panics.
//!
//! Refs: I-Core-NoPanic, I-Core-Pure

use brioche_core::{
    AfterPrediction, AgentState, BeforePrediction, BriocheEngineBuilder, ChatMessage,
    DecisionAggregator, Effect, EngineInput, ExecutionPath, ExtensionStorage,
    MAX_STATE_STACK_DEPTH, OnInput, OnStreamEvent, OnToolCalls, OnToolResult, PluginResult,
    PolicyDecision, Session, StreamAction, StreamEvent, SubRoutineHandle, SubRoutineLifecycleGuard,
    ToolCallDescriptor, ToolResultDTO,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Mock governance traits
// ---------------------------------------------------------------------------

struct MockDecisionAggregator;
impl DecisionAggregator for MockDecisionAggregator {
    type PolicyDecision = PolicyDecision;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
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
    type SubRoutineHandle = SubRoutineHandle;
    type Session = Session;
    type SessionRegistry = brioche_core::SessionRegistry;
    type Effect = Effect;
    type PluginError = brioche_core::PluginError;
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
        builder = register_pure_plugin_a(builder);
        builder = register_pure_plugin_b(builder);
    } else {
        builder = register_pure_plugin_b(builder);
        builder = register_pure_plugin_a(builder);
    }
    builder.build()
}

// ---------------------------------------------------------------------------
// Pure plugins for order-independence tests
// ---------------------------------------------------------------------------

/// Pure plugin A — always Allow, no side effects.
struct PurePluginA;

impl OnInput for PurePluginA {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_a"
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

impl BeforePrediction for PurePluginA {
    type ChatMessage = ChatMessage;
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_a"
    }

    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

impl OnStreamEvent for PurePluginA {
    type StreamEvent = StreamEvent;
    type ExtensionStorage = ExtensionStorage;
    type StreamAction = StreamAction;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_a"
    }

    fn on_stream_event(
        &self,
        _event: &StreamEvent,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        Ok(StreamAction::Pass)
    }
}

impl AfterPrediction for PurePluginA {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_a"
    }

    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }
}

impl OnToolCalls for PurePluginA {
    type ToolCallDescriptor = ToolCallDescriptor;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_a"
    }

    fn on_tool_calls(
        &self,
        _calls: &mut Vec<ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }
}

impl OnToolResult for PurePluginA {
    type ToolResultDto = ToolResultDTO;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_a"
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

impl OnInput for PurePluginB {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_b"
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

impl BeforePrediction for PurePluginB {
    type ChatMessage = ChatMessage;
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_b"
    }

    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

impl OnStreamEvent for PurePluginB {
    type StreamEvent = StreamEvent;
    type ExtensionStorage = ExtensionStorage;
    type StreamAction = StreamAction;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_b"
    }

    fn on_stream_event(
        &self,
        _event: &StreamEvent,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        Ok(StreamAction::Pass)
    }
}

impl AfterPrediction for PurePluginB {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_b"
    }

    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }
}

impl OnToolCalls for PurePluginB {
    type ToolCallDescriptor = ToolCallDescriptor;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_b"
    }

    fn on_tool_calls(
        &self,
        _calls: &mut Vec<ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }
}

impl OnToolResult for PurePluginB {
    type ToolResultDto = ToolResultDTO;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "pure_b"
    }

    fn on_tool_result(
        &self,
        _results: &mut Vec<ToolResultDTO>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }
}

fn register_pure_plugin_a<DA, LG>(
    builder: BriocheEngineBuilder<DA, LG>,
) -> BriocheEngineBuilder<DA, LG> {
    builder
        .with_on_input(Box::new(PurePluginA))
        .with_before_prediction(Box::new(PurePluginA))
        .with_on_stream_event(Box::new(PurePluginA))
        .with_after_prediction(Box::new(PurePluginA))
        .with_on_tool_calls(Box::new(PurePluginA))
        .with_on_tool_result(Box::new(PurePluginA))
}

fn register_pure_plugin_b<DA, LG>(
    builder: BriocheEngineBuilder<DA, LG>,
) -> BriocheEngineBuilder<DA, LG> {
    builder
        .with_on_input(Box::new(PurePluginB))
        .with_before_prediction(Box::new(PurePluginB))
        .with_on_stream_event(Box::new(PurePluginB))
        .with_after_prediction(Box::new(PurePluginB))
        .with_on_tool_calls(Box::new(PurePluginB))
        .with_on_tool_result(Box::new(PurePluginB))
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

// ---------------------------------------------------------------------------
// AgentState sequence property tests
// ---------------------------------------------------------------------------

/// A single operation in a randomized `AgentState` sequence.
///
/// Sequences mix Core mechanism calls (`push_state`, `pop_state`, direct
/// state replacement) with full engine transitions. This exposes
/// interactions that isolated unit tests cannot reach.
#[derive(Debug, Clone)]
enum AgentStateOp {
    /// Push a state onto the hierarchical stack.
    Push(AgentState),
    /// Pop the top state from the stack.
    Pop,
    /// Replace the current state without touching the stack.
    Replace(AgentState),
    /// Run a full `EngineInput` transition.
    Transition(EngineInput),
}

fn agent_state_strategy() -> impl Strategy<Value = AgentState> {
    prop_oneof![
        Just(AgentState::Idle),
        (1u64..100u64).prop_map(|generation_id| AgentState::Predicting { generation_id }),
        (1u64..100u64).prop_map(|generation_id| AgentState::ExecutingTools { generation_id }),
        "[a-z0-9]{1,8}".prop_map(|id| AgentState::SubRoutine(SubRoutineHandle::new(id))),
        Just(AgentState::Failure),
    ]
}

fn agent_state_op_strategy() -> impl Strategy<Value = AgentStateOp> {
    prop_oneof![
        agent_state_strategy().prop_map(AgentStateOp::Push),
        Just(AgentStateOp::Pop),
        agent_state_strategy().prop_map(AgentStateOp::Replace),
        engine_input_strategy().prop_map(AgentStateOp::Transition),
    ]
}

/// Parse a base-10 non-negative integer at compile time.
const fn parse_u32(s: &str) -> Option<u32> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut value: u32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < b'0' || b > b'9' {
            return None;
        }
        value = value.wrapping_mul(10).wrapping_add((b - b'0') as u32);
        i += 1;
    }
    Some(value)
}

/// Configurable case count for the AgentState sequence proptests.
///
/// Defaults to 1,000 cases and can be overridden via the
/// `AGENTSTATE_PROPTEST_CASES` environment variable at compile time.
const AGENTSTATE_PROPTEST_CASES: u32 = match option_env!("AGENTSTATE_PROPTEST_CASES") {
    Some(s) => match parse_u32(s) {
        Some(n) => n,
        None => 1_000,
    },
    None => 1_000,
};

/// Apply a sequence of operations and check every listed invariant.
///
/// Invariants verified:
/// - Stack depth never exceeds `MAX_STATE_STACK_DEPTH`.
/// - `generation_id` is monotonically non-decreasing across valid
///   `UserMessage` transitions that reach `Predicting`.
/// - `AgentState::Failure` rejects further engine inputs.
/// - `pop_state` on an empty stack returns `Err`, never panics.
fn run_agent_state_sequence(
    engine: &mut brioche_core::BriocheEngine,
    session: &mut Session,
    ops: &[AgentStateOp],
) -> Result<(), TestCaseError> {
    let mut max_observed_generation: Option<u64> = None;

    for op in ops {
        match op {
            AgentStateOp::Push(state) => {
                let prev_depth = session.state_stack.len();
                let result = session.push_state(state.clone());
                prop_assert!(
                    session.state_stack.len() <= MAX_STATE_STACK_DEPTH,
                    "stack depth {} exceeds maximum {}",
                    session.state_stack.len(),
                    MAX_STATE_STACK_DEPTH
                );
                if result.is_ok() {
                    prop_assert_eq!(
                        session.state_stack.len(),
                        prev_depth + 1,
                        "successful push must increase depth by one"
                    );
                }
            }
            AgentStateOp::Pop => {
                let prev_depth = session.state_stack.len();
                let result = session.pop_state();
                if prev_depth == 0 {
                    prop_assert!(result.is_err(), "pop on empty stack must return Err");
                } else {
                    prop_assert!(result.is_ok(), "pop on non-empty stack must succeed");
                    prop_assert_eq!(
                        session.state_stack.len(),
                        prev_depth - 1,
                        "successful pop must decrease depth by one"
                    );
                }
            }
            AgentStateOp::Replace(state) => {
                session.state = state.clone();
            }
            AgentStateOp::Transition(input) => {
                let was_failure = matches!(session.state, AgentState::Failure);
                let effects = engine.transition(session, input);

                if was_failure {
                    prop_assert!(
                        matches!(session.state, AgentState::Failure),
                        "Failure state must reject further inputs, got {:?}",
                        session.state
                    );
                    prop_assert!(
                        effects
                            .iter()
                            .any(|effect| matches!(effect, Effect::Error { .. })),
                        "Failure rejection must emit an error effect"
                    );
                    continue;
                }

                // Track generation_id monotonicity across UserMessage transitions.
                if matches!(input, EngineInput::UserMessage(_))
                    && let AgentState::Predicting { generation_id } = session.state
                {
                    if let Some(max_gen) = max_observed_generation {
                        prop_assert!(
                            generation_id >= max_gen,
                            "generation_id {} decreased below previous maximum {}",
                            generation_id,
                            max_gen
                        );
                    }
                    let next_max = match max_observed_generation {
                        Some(max_gen) => max_gen.max(generation_id),
                        None => generation_id,
                    };
                    max_observed_generation = Some(next_max);
                }
            }
        }

        // Global invariant: depth is always bounded, regardless of operation.
        prop_assert!(
            session.state_stack.len() <= MAX_STATE_STACK_DEPTH,
            "stack depth {} exceeds maximum {}",
            session.state_stack.len(),
            MAX_STATE_STACK_DEPTH
        );
    }

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(AGENTSTATE_PROPTEST_CASES))]

    #[test]
    fn prop_agent_state_sequence_invariants(
        ops in prop::collection::vec(agent_state_op_strategy(), 0..32)
    ) {
        let mut engine = build_engine();
        let mut session = Session::new("prop-agent-state");
        run_agent_state_sequence(&mut engine, &mut session, &ops)?;
    }
}
