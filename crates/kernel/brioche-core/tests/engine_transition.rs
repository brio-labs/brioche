//! Book I — Sprint 4 integration tests: `BriocheEngine`, `UnifiedRoutingTable`,
//! and `transition()`.
//!
//! Invariants verified:
//! - I-Core-StreamNoBranch: pre-routed dispatch eliminates hot-path branching.
//! - I-Core-PluginOrder: total order via `priority` + `name`.
//! - I-Core-NoPanic: `transition()` returns `Vec<Effect>`, never panics.
//! - I-Core-RetVecEffect: all side effects are returned as `Effect`.

use brioche_core::{
    AgentState, BriocheEngineBuilder, BriocheExtensionType, BriochePlugin, ChatMessage,
    ConsistencyVerifier, CycleRollbackPolicy, DecisionAggregator, Effect, EngineInput, EpochAction,
    EpochInterceptor, ErrorCode, ExecutionPath, ExtensionStorage, HistoryEdit, PluginCapabilities,
    PluginResult, PolicyDecision, Session, SessionRegistry, StreamEvent, SubRoutineHandle,
    SubRoutineLifecycleGuard, ToolCallDescriptor, ToolResultDTO, UnifiedRoutingTable,
};

// ---------------------------------------------------------------------------
// Mandatory trait mocks
// ---------------------------------------------------------------------------

struct MockDecisionAggregator;

impl DecisionAggregator for MockDecisionAggregator {
    fn aggregate_decisions(
        &self,
        decisions: Vec<PolicyDecision>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        // First Block wins; accumulate MutateHistory; OverrideTransition/RequestEffect short-circuit.
        let mut edits = Vec::new();
        for d in decisions {
            match d {
                PolicyDecision::Block { reason } => {
                    return Ok(PolicyDecision::Block { reason });
                }
                PolicyDecision::MutateHistory(mut e) => {
                    edits.append(&mut e);
                }
                PolicyDecision::OverrideTransition(eff) => {
                    return Ok(PolicyDecision::OverrideTransition(eff));
                }
                PolicyDecision::RequestEffect(eff) => {
                    return Ok(PolicyDecision::RequestEffect(eff));
                }
                PolicyDecision::Allow => {}
            }
        }
        if edits.is_empty() {
            Ok(PolicyDecision::Allow)
        } else {
            Ok(PolicyDecision::MutateHistory(edits))
        }
    }
}

struct MockSubRoutineLifecycleGuard;

impl SubRoutineLifecycleGuard for MockSubRoutineLifecycleGuard {
    fn on_exit(
        &self,
        _handle: SubRoutineHandle,
        _parent: &mut Session,
        registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        registry.increment_exit_count(&SubRoutineHandle::new("mock"));
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Builder tests
// ---------------------------------------------------------------------------

#[test]
fn builder_missing_mandatory_traits_fails() {
    let builder = BriocheEngineBuilder::new();
    let result = builder.build();
    assert!(result.is_err());
    let err = match result {
        Err(e) => e,
        Ok(_) => {
            assert_eq!(1, 0, "expected error");
            return;
        }
    };
    assert!(err.to_string().contains("DecisionAggregator"));

    let builder =
        BriocheEngineBuilder::new().with_decision_aggregator(Box::new(MockDecisionAggregator));
    let result = builder.build();
    assert!(result.is_err());
    let err = match result {
        Err(e) => e,
        Ok(_) => {
            assert_eq!(1, 0, "expected error");
            return;
        }
    };
    assert!(err.to_string().contains("SubRoutineLifecycleGuard"));
}

#[test]
fn builder_with_all_mandatory_traits_succeeds() {
    let result = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// UserMessage dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_user_message_to_predicting() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    // State should be Predicting.
    assert!(matches!(session.state, AgentState::Predicting { .. }));
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));

    // Effects should include CallLlmNetwork and SaveSession.
    assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
    assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
}

#[test]
fn transition_user_message_generates_generation_id() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    engine.transition(&mut session, &EngineInput::UserMessage("a".into()));

    let gen_a = match session.state {
        AgentState::Predicting { generation_id } => generation_id,
        _ => {
            assert_eq!(1, 0, "expected Predicting");
            return;
        }
    };

    // Pop state to Idle so next transition works.
    let pop_result = session.pop_state();
    assert!(pop_result.is_ok());

    engine.transition(&mut session, &EngineInput::UserMessage("b".into()));
    let gen_b = match session.state {
        AgentState::Predicting { generation_id } => generation_id,
        _ => {
            assert_eq!(1, 0, "expected Predicting");
            return;
        }
    };

    assert!(
        gen_b > gen_a,
        "generation_id should be monotonically increasing"
    );
}

// ---------------------------------------------------------------------------
// LlmStream dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_llm_stream_in_predicting_routes_plugins() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(push_result.is_ok());

    let event = StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from_static(b"hi"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(event));

    // No plugins registered, so no effects besides defaults.
    assert!(effects.is_empty());
}

#[test]
fn transition_llm_stream_not_predicting_returns_empty() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let event = StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from_static(b"hi"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(event));

    assert!(effects.is_empty());
}

#[test]
fn transition_llm_stream_accumulates_assistant_text() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(push_result.is_ok());

    // Simulate streaming: two text chunks then Done.
    let chunk1 = StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from_static(b"Hello "),
    };
    let chunk2 = StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from_static(b"world"),
    };
    let done = StreamEvent::Done;

    let effects1 = engine.transition(&mut session, &EngineInput::LlmStream(chunk1));
    let effects2 = engine.transition(&mut session, &EngineInput::LlmStream(chunk2));
    let effects3 = engine.transition(&mut session, &EngineInput::LlmStream(done));

    // No effects from chunks themselves; Done triggers SystemIdle + SaveSession.
    assert!(effects1.is_empty());
    assert!(effects2.is_empty());
    assert!(effects3.iter().any(|e| matches!(e, Effect::SystemIdle)));
    assert!(effects3.iter().any(|e| matches!(e, Effect::SaveSession)));

    // Buffer should be empty after Done.
    assert!(session.pending_assistant_text.is_empty());

    // Assistant message should be in history.
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::Assistant { content, .. } if content == "Hello world"
    ));

    // State should have popped back to Idle.
    assert!(matches!(session.state, AgentState::Idle));
}

#[test]
fn transition_llm_stream_tool_call_done_persists_preceding_text() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(1000)
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(push_result.is_ok());

    // Assistant says something, then calls a tool.
    let text = StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from_static(b"Let me check"),
    };
    let start = StreamEvent::ToolCallStart {
        path: Default::default(),
        id: "tc1".into(),
        name: "calc".into(),
    };
    let done = StreamEvent::ToolCallDone {
        path: Default::default(),
    };

    engine.transition(&mut session, &EngineInput::LlmStream(text));
    engine.transition(&mut session, &EngineInput::LlmStream(start));
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(done));

    // Preceding text should be persisted as Assistant message.
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::Assistant { content, .. } if content == "Let me check"
    ));

    // Buffer cleared.
    assert!(session.pending_assistant_text.is_empty());

    // State transitions to ExecutingTools.
    assert!(matches!(
        session.state,
        AgentState::ExecutingTools { generation_id: 1 }
    ));
    assert!(effects.iter().any(|e| matches!(e, Effect::ExecuteTools(_))));
}

// ---------------------------------------------------------------------------
// ToolCallsResult dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_tool_calls_result_pops_state() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let r1 = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(r1.is_ok());
    let r2 = session.push_state(AgentState::ExecutingTools { generation_id: 1 });
    assert!(r2.is_ok());

    let result = ToolResultDTO {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        outcome: brioche_core::ToolOutcome::Success("42".into()),
    };
    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 1,
            results: vec![result],
        },
    );

    // Should pop back to Predicting.
    assert!(matches!(session.state, AgentState::Predicting { .. }));
    // History should contain the tool result.
    assert!(session.history.iter().any(|m| matches!(
        m, ChatMessage::ToolResult { id, .. } if id == "t1"
    )));
    // Effects should include CallLlmNetwork + SaveSession.
    assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
    assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
}

// ---------------------------------------------------------------------------
// RestoreSubRoutine dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_restore_subroutine_registers_in_registry() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let handle = SubRoutineHandle::new("sub-1");
    let effects = engine.transition(
        &mut session,
        &EngineInput::RestoreSubRoutine {
            handle: handle.clone(),
            head_blob: vec![],
        },
    );

    assert!(engine.session_registry().contains(&handle));
    assert!(effects.iter().any(|e| matches!(
        e, Effect::SubRoutineRestored { handle: h } if h == &handle
    )));
    assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
}

// ---------------------------------------------------------------------------
// Plugin routing and ordering
// ---------------------------------------------------------------------------

struct PriorityTestPlugin {
    name: &'static str,
    priority: i16,
    cap: PluginCapabilities,
}

impl BriochePlugin for PriorityTestPlugin {
    fn name(&self) -> &'static str {
        self.name
    }
    fn capabilities(&self) -> PluginCapabilities {
        self.cap
    }
    fn priority(&self) -> i16 {
        self.priority
    }
}

#[test]
fn routing_table_orders_by_priority_then_name() {
    let plugins: Vec<Box<dyn BriochePlugin>> = vec![
        Box::new(PriorityTestPlugin {
            name: "beta",
            priority: 0,
            cap: PluginCapabilities::ON_INPUT,
        }),
        Box::new(PriorityTestPlugin {
            name: "alpha",
            priority: 0,
            cap: PluginCapabilities::ON_INPUT,
        }),
        Box::new(PriorityTestPlugin {
            name: "gamma",
            priority: -1,
            cap: PluginCapabilities::ON_INPUT,
        }),
    ];

    let table = UnifiedRoutingTable::from_plugins(&plugins);

    // Expected order: gamma (-1), alpha (0, "alpha" < "beta"), beta (0).
    assert_eq!(table.route_on_input, vec![2, 1, 0]);
}

#[test]
fn routing_table_filters_by_capability() {
    let plugins: Vec<Box<dyn BriochePlugin>> = vec![
        Box::new(PriorityTestPlugin {
            name: "input_only",
            priority: 0,
            cap: PluginCapabilities::ON_INPUT,
        }),
        Box::new(PriorityTestPlugin {
            name: "stream_only",
            priority: 0,
            cap: PluginCapabilities::ON_STREAM_EVENT,
        }),
    ];

    let table = UnifiedRoutingTable::from_plugins(&plugins);

    assert_eq!(table.route_on_input, vec![0]);
    assert_eq!(table.route_on_stream_event, vec![1]);
    assert!(table.route_before_prediction.is_empty());
}

// ---------------------------------------------------------------------------
// OverrideTransition and Block on on_input
// ---------------------------------------------------------------------------

struct OverrideInputPlugin;

impl BriochePlugin for OverrideInputPlugin {
    fn name(&self) -> &'static str {
        "override_input"
    }
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }
    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::OverrideTransition(vec![
            Effect::ForwardToUi(brioche_core::UiWidget::Test {
                msg: "overridden".to_string(),
            }),
        ]))
    }
}

#[test]
fn transition_override_input_short_circuits() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_plugin(Box::new(OverrideInputPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    // Should NOT transition to Predicting because on_input short-circuited.
    assert!(matches!(session.state, AgentState::Idle));
    assert!(effects.iter().any(|e| matches!(
        e, Effect::ForwardToUi(widget) if widget.widget_type() == "test"
    )));
}

struct BlockInputPlugin;

impl BriochePlugin for BlockInputPlugin {
    fn name(&self) -> &'static str {
        "block_input"
    }
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }
    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Block {
            reason: "blocked".into(),
        })
    }
}

#[test]
fn transition_block_input_returns_error_and_idle() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_plugin(Box::new(BlockInputPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(matches!(session.state, AgentState::Idle));
    assert!(effects.iter().any(|e| matches!(
        e, Effect::Error { code, .. } if *code == ErrorCode::StateInconsistency
    )));
    assert!(effects.iter().any(|e| matches!(e, Effect::SystemIdle)));
}

// ---------------------------------------------------------------------------
// EpochInterceptor
// ---------------------------------------------------------------------------

struct BlockEpochInterceptor;

impl EpochInterceptor for BlockEpochInterceptor {
    fn intercept_epoch(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<EpochAction> {
        Ok(EpochAction::Block {
            reason: "epoch stale".into(),
        })
    }
}

#[test]
fn transition_epoch_block_returns_error_idle() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_epoch_interceptor(Box::new(BlockEpochInterceptor))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(effects.iter().any(|e| matches!(
        e, Effect::Error { code, .. } if *code == ErrorCode::EpochMismatch
    )));
    assert!(effects.iter().any(|e| matches!(e, Effect::SystemIdle)));
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn transition_is_deterministic() {
    let mut engine_a = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut engine_b = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session_a = Session::new("det");
    let mut session_b = Session::new("det");

    let effects_a = engine_a.transition(&mut session_a, &EngineInput::UserMessage("x".into()));
    let effects_b = engine_b.transition(&mut session_b, &EngineInput::UserMessage("x".into()));

    assert_eq!(effects_a, effects_b);
    assert_eq!(session_a.state, session_b.state);
}

// ---------------------------------------------------------------------------
// SubRoutineLifecycleGuard
// ---------------------------------------------------------------------------

#[test]
fn transition_exits_subroutine_triggers_lifecycle_guard() {
    struct CountingLifecycleGuard;
    impl SubRoutineLifecycleGuard for CountingLifecycleGuard {
        fn on_exit(
            &self,
            _handle: SubRoutineHandle,
            _parent: &mut Session,
            registry: &mut SessionRegistry,
        ) -> PluginResult<Vec<Effect>> {
            registry.increment_exit_count(&SubRoutineHandle::new("counted"));
            Ok(vec![Effect::SaveSession])
        }
    }

    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(CountingLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let r = session.push_state(AgentState::Idle);
    assert!(r.is_ok());
    session.state = AgentState::SubRoutine(SubRoutineHandle::new("sub-1"));

    // Simulate a ToolCallsResult that pops state back to Idle.
    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 1,
            results: vec![],
        },
    );

    // Lifecycle guard should have been called (added SaveSession).
    assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
}

// ---------------------------------------------------------------------------
// ConsistencyVerifier
// ---------------------------------------------------------------------------

#[test]
fn transition_consistency_verifier_effects_appended() {
    struct ForcingVerifier;
    impl ConsistencyVerifier for ForcingVerifier {
        fn verify_consistency(&self, _session: &mut Session) -> PluginResult<Option<Vec<Effect>>> {
            Ok(Some(vec![Effect::SystemIdle]))
        }
    }

    let mut engine = match BriocheEngineBuilder::new()
        .with_consistency_verifier(Box::new(ForcingVerifier))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(effects.iter().any(|e| matches!(e, Effect::SystemIdle)));
}

// ---------------------------------------------------------------------------
// RebuildRoutes position guarantee
// ---------------------------------------------------------------------------

#[test]
fn transition_rebuildroutes_is_last() {
    struct RebuildPlugin;
    impl BriochePlugin for RebuildPlugin {
        fn name(&self) -> &'static str {
            "rebuild"
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::ON_INPUT
        }
        fn on_input(
            &self,
            _input: &EngineInput,
            _ext: &mut ExtensionStorage,
        ) -> PluginResult<PolicyDecision> {
            Ok(PolicyDecision::OverrideTransition(vec![
                Effect::RebuildRoutes,
                Effect::SaveSession, // This should be truncated
            ]))
        }
    }

    let mut engine = match BriocheEngineBuilder::new()
        .with_plugin(Box::new(RebuildPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(matches!(effects.last(), Some(Effect::RebuildRoutes)));
    assert_eq!(effects.len(), 1);
}

// ---------------------------------------------------------------------------
// HistoryEdit application
// ---------------------------------------------------------------------------

#[test]
fn transition_history_edit_insert_and_truncate() {
    struct EditPlugin;
    impl BriochePlugin for EditPlugin {
        fn name(&self) -> &'static str {
            "edit"
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::BEFORE_PREDICTION
        }
        fn before_prediction(
            &self,
            _history: &[ChatMessage],
            _ext: &mut ExtensionStorage,
        ) -> PluginResult<PolicyDecision> {
            Ok(PolicyDecision::MutateHistory(vec![HistoryEdit::Insert {
                index: 0,
                message: ChatMessage::System {
                    content: "injected".into(),
                },
            }]))
        }
    }

    let mut engine = match BriocheEngineBuilder::new()
        .with_plugin(Box::new(EditPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
    assert_eq!(session.history.len(), 2);
    assert!(matches!(
        &session.history[0],
        ChatMessage::System { content } if content == "injected"
    ));
    assert!(matches!(
        &session.history[1],
        ChatMessage::User { content } if content == "hello"
    ));
}

// ---------------------------------------------------------------------------
// Sprint 5: seal() integration, ActiveToolCall materialization, EngineInput
// dispatch refinement
// ---------------------------------------------------------------------------

#[test]
fn transition_llm_stream_tool_call_materialization() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(5000)
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 7 });
    assert!(push_result.is_ok());

    // Send ToolCallStart
    let start = StreamEvent::ToolCallStart {
        path: ExecutionPath::default(),
        id: "tc1".into(),
        name: "calc".into(),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(start));
    assert!(effects.is_empty());
    assert!(matches!(session.state, AgentState::Predicting { .. }));

    // Send argument chunk
    let arg = StreamEvent::ToolArgumentChunk {
        path: ExecutionPath::default(),
        id: "tc1".into(),
        chunk: bytes::Bytes::from_static(b"{\"x\":1}"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(arg));
    assert!(effects.is_empty());

    // Send ToolCallDone -> materialization
    let done = StreamEvent::ToolCallDone {
        path: ExecutionPath::default(),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(done));

    // State should transition to ExecutingTools.
    assert!(matches!(
        session.state,
        AgentState::ExecutingTools { generation_id: 7 }
    ));

    // active_tools should contain the sealed call.
    assert_eq!(session.active_tools.len(), 1);
    assert_eq!(session.active_tools[0].tool_id, "tc1");
    assert_eq!(session.active_tools[0].tool_name, "calc");
    assert_eq!(session.active_tools[0].arguments, "{\"x\":1}");
    assert_eq!(session.active_tools[0].timeout_ms, 5000);

    // Effects should include ExecuteTools and SaveSession.
    assert!(effects.iter().any(|e| matches!(e, Effect::ExecuteTools(_))));
    assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
}

#[test]
fn transition_llm_stream_missing_timeout_applies_default() {
    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(3000)
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(push_result.is_ok());

    let start = StreamEvent::ToolCallStart {
        path: ExecutionPath::default(),
        id: "t1".into(),
        name: "grep".into(),
    };
    engine.transition(&mut session, &EngineInput::LlmStream(start));

    let done = StreamEvent::ToolCallDone {
        path: ExecutionPath::default(),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(done));

    // Default timeout applied silently (no error). A missing timeout
    // is now a normal condition: the kernel applies the default and
    // lets governance plugins (if any) enforce stricter policies.
    assert_eq!(session.active_tools[0].timeout_ms, 3000);

    // No spurious error for missing timeout.
    assert!(!effects.iter().any(|e| matches!(
        e, Effect::Error { message, .. } if message.contains("Missing timeout")
    )));
}

#[test]
fn transition_llm_stream_on_tool_calls_mutates_timeout() {
    struct TimeoutMutatorPlugin;
    impl BriochePlugin for TimeoutMutatorPlugin {
        fn name(&self) -> &'static str {
            "timeout_mutator"
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::ON_TOOL_CALLS
        }
        fn on_tool_calls(
            &self,
            calls: &mut Vec<ToolCallDescriptor>,
            _ext: &mut ExtensionStorage,
        ) -> PluginResult<()> {
            for call in calls {
                call.timeout_ms = Some(9999);
            }
            Ok(())
        }
    }

    let mut engine = match BriocheEngineBuilder::new()
        .with_plugin(Box::new(TimeoutMutatorPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(1000)
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 2 });
    assert!(push_result.is_ok());

    let start = StreamEvent::ToolCallStart {
        path: ExecutionPath::default(),
        id: "t2".into(),
        name: "calc".into(),
    };
    engine.transition(&mut session, &EngineInput::LlmStream(start));

    let done = StreamEvent::ToolCallDone {
        path: ExecutionPath::default(),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(done));

    // Plugin should have mutated timeout to 9999.
    assert_eq!(session.active_tools[0].timeout_ms, 9999);

    // No error effect because timeout was provided by plugin.
    assert!(!effects.iter().any(|e| matches!(
        e, Effect::Error { message, .. } if message.contains("Missing timeout")
    )));
}

// ---------------------------------------------------------------------------
// Sprint 6: Fundamental governance traits — default implementations
// ---------------------------------------------------------------------------

#[test]
fn transition_with_epoch_guard_blocks_stale_generation() {
    use brioche_governance_default::EpochGuard;

    let mut engine = match BriocheEngineBuilder::new()
        .with_epoch_interceptor(Box::new(EpochGuard))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    // Simulate an advanced epoch
    session.extensions.insert(brioche_core::EpochState {
        current_generation: 5,
    });

    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 3, // obsolete
            results: vec![],
        },
    );

    assert!(effects.iter().any(|e| matches!(
        e, Effect::Error { code, .. } if *code == ErrorCode::EpochMismatch
    )));
    assert!(effects.iter().any(|e| matches!(e, Effect::SystemIdle)));
}

#[test]
fn transition_with_epoch_guard_allows_current_generation() {
    use brioche_governance_default::EpochGuard;

    let mut engine = match BriocheEngineBuilder::new()
        .with_epoch_interceptor(Box::new(EpochGuard))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    session.extensions.insert(brioche_core::EpochState {
        current_generation: 7,
    });

    let r1 = session.push_state(AgentState::Predicting { generation_id: 7 });
    assert!(r1.is_ok());
    let r2 = session.push_state(AgentState::ExecutingTools { generation_id: 7 });
    assert!(r2.is_ok());

    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 7,
            results: vec![],
        },
    );

    // No epoch error — normal processing continues.
    assert!(!effects.iter().any(|e| matches!(
        e, Effect::Error { code, .. } if *code == ErrorCode::EpochMismatch
    )));
}

#[test]
fn transition_with_policy_aggregator_allows() {
    use brioche_governance_default::LexicographicDecisionAggregator;

    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
    assert!(matches!(session.state, AgentState::Predicting { .. }));
}

#[test]
fn transition_with_subroutine_cleanup_guard_removes_child() {
    use brioche_governance_default::SubRoutineCleanupGuard;

    let mut engine = match BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let r = session.push_state(AgentState::Idle);
    assert!(r.is_ok());
    session.state = AgentState::SubRoutine(SubRoutineHandle::new("sub-1"));

    // Enregistrer la sous-routine dans le registry
    engine
        .session_registry_mut()
        .insert(SubRoutineHandle::new("sub-1"), Session::new("sub-1"));

    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 1,
            results: vec![],
        },
    );

    // The sub-routine should have been removed from the registry.
    assert!(
        !engine
            .session_registry()
            .contains(&SubRoutineHandle::new("sub-1"))
    );
    assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
}

#[test]
fn transition_with_state_consistency_guard_fixes_inconsistent_state() {
    use brioche_governance_default::StateConsistencyGuard;

    let mut engine = match BriocheEngineBuilder::new()
        .with_consistency_verifier(Box::new(StateConsistencyGuard::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    // Force an inconsistent state: Predicting without stack.
    session.state = AgentState::Predicting { generation_id: 1 };

    // LlmStream does not modify the stack when already in Predicting.
    let event = StreamEvent::TextChunk {
        path: ExecutionPath::default(),
        chunk: bytes::Bytes::from_static(b"x"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(event));

    // The guard should have forced a return to Idle.
    assert!(matches!(session.state, AgentState::Idle));
    assert!(session.state_stack.is_empty());
    assert!(effects.iter().any(|e| matches!(e, Effect::SystemIdle)));
}

#[test]
fn transition_with_fast_hook_effect_constraint_blocks_disallowed_effect() {
    use brioche_core::EffectBit;
    use brioche_governance_default::FastHookEffectConstraint;

    // Interdit tout sur le hook transition (index 0) sauf Error et SystemIdle.
    let mut masks = [0u64; 8];
    masks[0] = EffectBit::ERROR | EffectBit::SYSTEM_IDLE;
    let constraint = FastHookEffectConstraint::new(masks);

    let mut engine = match BriocheEngineBuilder::new()
        .with_hook_effect_constraint(Box::new(constraint))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    // CallLlmNetwork should be replaced by an error because it is disallowed.
    assert!(!effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
    assert!(effects.iter().any(|e| matches!(
        e, Effect::Error { code, .. } if *code == ErrorCode::StateInconsistency
    )));
}

#[test]
fn transition_with_system_failover_guard_replaces_fault() {
    use brioche_governance_default::SystemFailoverGuard;

    struct FaultyPlugin;
    impl BriochePlugin for FaultyPlugin {
        fn name(&self) -> &'static str {
            "faulty"
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::ON_INPUT
        }
        fn on_input(
            &self,
            _input: &EngineInput,
            _ext: &mut ExtensionStorage,
        ) -> PluginResult<PolicyDecision> {
            Err(brioche_core::PluginError::Fatal {
                plugin_name: "faulty".into(),
                message: "boom".into(),
            })
        }
    }

    let mut engine = match BriocheEngineBuilder::new()
        .with_plugin(Box::new(FaultyPlugin))
        .with_governance_failover_handler(Box::new(SystemFailoverGuard::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    // The failover should have replaced the fault with ForwardToUi + SystemIdle.
    assert!(effects.iter().any(|e| matches!(
        e, Effect::ForwardToUi(widget) if widget.widget_type() == "critical_error"
    )));
}

// ---------------------------------------------------------------------------
// Sprint 7: Optional traits + COW integration
// ---------------------------------------------------------------------------

/// Non-critical test type to validate the COW threshold.
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
pub struct TestCowState {
    pub value: u64,
}

#[test]
fn undo_frame_guard_restores_mutated_extension() {
    use brioche_governance_default::UndoFrameGuard;

    let mut guard = UndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    ext.insert(brioche_core::EpochState {
        current_generation: 42,
    });

    guard.begin_hook();

    // Snapshot the current value via on_mutation.
    let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
    let vtable = brioche_core::EpochState::build_vtable();
    let current = ext.get_or_insert_default::<brioche_core::EpochState>();
    guard.on_mutation(type_id, &vtable, current);

    // Mutate the extension.
    current.current_generation = 99;

    // Rollback should restore the original value.
    guard.rollback_hook(&mut ext);

    let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
    assert_eq!(restored.current_generation, 42);
}

#[test]
fn undo_frame_guard_abandons_past_threshold() {
    use brioche_governance_default::UndoFrameGuard;

    let mut guard = UndoFrameGuard::with_max_cow_bytes(0); // 0 byte threshold
    let mut ext = ExtensionStorage::new();
    ext.insert(TestCowState { value: 7 });

    guard.begin_hook();

    let type_id = std::any::TypeId::of::<TestCowState>();
    let vtable = TestCowState::build_vtable();
    let current = ext.get_or_insert_default::<TestCowState>();
    guard.on_mutation(type_id, &vtable, current);

    // Mutation abandoned due to threshold — state won't be restored.
    current.value = 123;

    guard.rollback_hook(&mut ext);

    let not_restored = ext.get_or_insert_default::<TestCowState>();
    assert_eq!(not_restored.value, 123);
}

#[test]
fn tool_execution_tracker_counts_outcomes() {
    use brioche_governance_default::{ToolExecutionTelemetry, ToolExecutionTracker};

    let tracker = ToolExecutionTracker::new();
    let mut ext = ExtensionStorage::new();

    // Simulate two tool calls.
    let mut calls = vec![
        ToolCallDescriptor {
            tool_id: "t1".into(),
            tool_name: "calc".into(),
            arguments: "{}".into(),
            timeout_ms: Some(1000),
        },
        ToolCallDescriptor {
            tool_id: "t2".into(),
            tool_name: "grep".into(),
            arguments: "{}".into(),
            timeout_ms: Some(2000),
        },
    ];
    assert!(tracker.on_tool_calls(&mut calls, &mut ext).is_ok());

    let state = ext.get_or_insert_default::<ToolExecutionTelemetry>();
    assert_eq!(state.start_timestamps.len(), 2);
    assert!(state.start_timestamps.contains_key("t1"));
    assert!(state.start_timestamps.contains_key("t2"));

    // Simulate results: one success, one failure.
    let mut results = vec![
        ToolResultDTO {
            tool_id: "t1".into(),
            tool_name: "calc".into(),
            outcome: brioche_core::ToolOutcome::Success("42".into()),
        },
        ToolResultDTO {
            tool_id: "t2".into(),
            tool_name: "grep".into(),
            outcome: brioche_core::ToolOutcome::SystemError("not found".into()),
        },
    ];
    assert!(tracker.on_tool_result(&mut results, &mut ext).is_ok());

    let state = ext.get_or_insert_default::<ToolExecutionTelemetry>();
    assert_eq!(state.completed_count, 1);
    assert_eq!(state.failed_count, 1);
    assert!(state.start_timestamps.is_empty());
}

#[test]
fn engine_with_undo_frame_guard_instruments_hooks() {
    use brioche_governance_default::UndoFrameGuard;

    /// Test-only extension type for verifying COW rollback behavior.
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
    struct TestCounter {
        value: u64,
    }

    struct MutatingPlugin;
    impl BriochePlugin for MutatingPlugin {
        fn name(&self) -> &'static str {
            "mutating"
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::ON_INPUT
        }
        fn on_input(
            &self,
            _input: &EngineInput,
            ext: &mut ExtensionStorage,
        ) -> PluginResult<PolicyDecision> {
            let state = ext.get_or_insert_default::<TestCounter>();
            state.value = 999;
            Ok(PolicyDecision::Allow)
        }
    }

    let mut engine = match BriocheEngineBuilder::new()
        .with_plugin(Box::new(MutatingPlugin))
        .with_cycle_rollback_policy(Box::new(UndoFrameGuard::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            assert_eq!(1, 0, "build failed: {}", err);
            return;
        }
    };

    let mut session = Session::new("test");
    session.extensions.insert(TestCounter { value: 1 });

    // The hook mutates TestCounter; COW instrumentation should not interfere
    // with normal operation (commit_hook is called when budget is respected).
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(matches!(session.state, AgentState::Predicting { .. }));
    assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));

    // The mutation should have been committed (not rolled back).
    let state = session.extensions.get_or_insert_default::<TestCounter>();
    assert_eq!(state.value, 999);
}
