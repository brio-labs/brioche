//! Book I — Sprint 4 integration tests: `BriocheEngine`, `UnifiedRoutingTable`,
//! and `transition()`.
//!
//! Invariants verified:
//! - I-Core-StreamNoBranch: pre-routed dispatch eliminates hot-path branching.
//! - I-Core-PluginOrder: total order via `priority` + `name`.
//! - I-Core-NoPanic: `transition()` returns `Vec<Effect>`, never panics.
//! - I-Core-RetVecEffect: all side effects are returned as `Effect`.

use brioche_core::{
    AgentState, BriocheEngineBuilder, BriochePlugin, ChatMessage, ConsistencyVerifier,
    DecisionAggregator, Effect, EngineInput, EpochAction, EpochInterceptor, ErrorCode,
    ExtensionStorage, HistoryEdit, PluginCapabilities, PluginResult, PolicyDecision, Session,
    SessionRegistry, StreamEvent, SubRoutineHandle, SubRoutineLifecycleGuard, ToolResultDTO,
    UnifiedRoutingTable,
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
            Effect::ForwardToUi {
                widget_type: "test".into(),
                payload: {
                    let mut map = serde_json::Map::new();
                    map.insert(
                        "msg".to_string(),
                        serde_json::Value::String("overridden".to_string()),
                    );
                    serde_json::Value::Object(map)
                },
            },
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
        e, Effect::ForwardToUi { widget_type, .. } if widget_type == "test"
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
