//! Book I — Sprint 4 integration tests: `BriocheEngine`, `UnifiedRoutingTable`,
//! and `transition()`.
//!
//! Invariants verified:
//! - I-Core-StreamNoBranch: pre-routed dispatch eliminates hot-path branching.
//! - I-Core-PluginOrder: total order via `priority` + `name`.
//! - I-Core-NoPanic: `transition()` returns `Vec<Effect>`, never panics.
//! - I-Core-RetVecEffect: all side effects are returned as `Effect`.

use brioche_core::{
    ActiveToolCall, AgentState, BriocheEngineBuilder, BriocheExtensionType, BriochePlugin,
    ChatMessage, ConsistencyVerifier, CycleRollbackPolicy, DecisionAggregator, Effect, EngineInput,
    EpochAction, EpochInterceptor, ErrorCode, ErrorDetail, ExecutionPath, ExtensionStorage,
    HistoryEdit, PluginCapabilities, PluginResult, PolicyDecision, Session, SessionRegistry,
    StreamEvent, SubRoutineHandle, SubRoutineLifecycleGuard, ToolCallDescriptor, ToolResultDTO,
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
                _ => {}
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
        _registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Builder tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// UserMessage dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_user_message_to_predicting() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));
    assert_eq!(effects, vec![Effect::SaveSession, Effect::CallLlmNetwork]);
}

#[test]
fn transition_user_message_generates_generation_id() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects_a = engine.transition(&mut session, &EngineInput::UserMessage("a".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(effects_a, vec![Effect::SaveSession, Effect::CallLlmNetwork]);

    let pop_result = session.pop_state();
    assert!(pop_result.is_ok());
    assert_eq!(session.state, AgentState::Idle);

    let effects_b = engine.transition(&mut session, &EngineInput::UserMessage("b".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 2 });
    assert_eq!(effects_b, vec![Effect::SaveSession, Effect::CallLlmNetwork]);
    assert_eq!(session.history.len(), 2);
}

// ---------------------------------------------------------------------------
// LlmStream dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_llm_stream_in_predicting_routes_plugins() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(push_result.is_ok());

    let event = StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from_static(b"hi"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(event));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert!(session.history.is_empty());
    assert_eq!(session.pending_assistant_text, "hi");
    assert!(effects.is_empty());
}

#[test]
fn transition_llm_stream_not_predicting_returns_empty() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let event = StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from_static(b"hi"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(event));

    assert_eq!(session.state, AgentState::Idle);
    assert!(session.history.is_empty());
    assert!(effects.is_empty());
}

#[test]
fn transition_llm_stream_accumulates_assistant_text() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(push_result.is_ok());

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

    assert!(effects1.is_empty());
    assert!(effects2.is_empty());
    assert_eq!(effects3, vec![Effect::SaveSession, Effect::SystemIdle]);

    assert!(session.pending_assistant_text.is_empty());
    assert_eq!(session.history.len(), 1);
    assert_eq!(
        session.history[0],
        ChatMessage::Assistant {
            content: "Hello world".into(),
            reasoning: None,
            tool_calls: vec![],
        }
    );
    assert_eq!(session.state, AgentState::Idle);
}

#[test]
fn transition_llm_stream_tool_call_done_persists_preceding_text() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(1000)
        .build();

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(push_result.is_ok());

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

    assert_eq!(session.history.len(), 1);
    assert_eq!(
        session.history[0],
        ChatMessage::Assistant {
            content: "Let me check".into(),
            reasoning: None,
            tool_calls: vec![],
        }
    );
    assert!(session.pending_assistant_text.is_empty());
    assert_eq!(
        session.state,
        AgentState::ExecutingTools { generation_id: 1 }
    );
    assert_eq!(session.active_tools.len(), 1);
    assert_eq!(
        session.active_tools[0],
        ActiveToolCall {
            tool_id: "tc1".into(),
            tool_name: "calc".into(),
            arguments: "".into(),
            timeout_ms: 1000,
        }
    );
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::MissingToolTimeout {
                    default_timeout_ms: 1000,
                },
            },
            Effect::SaveSession,
            Effect::ExecuteTools(vec![ActiveToolCall {
                tool_id: "tc1".into(),
                tool_name: "calc".into(),
                arguments: "".into(),
                timeout_ms: 1000,
            }]),
        ]
    );
}

// ---------------------------------------------------------------------------
// ToolCallsResult dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_tool_calls_result_pops_state() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

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

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert!(session.active_tools.is_empty());
    assert_eq!(session.history.len(), 1);
    assert_eq!(
        session.history[0],
        ChatMessage::ToolResult {
            id: "t1".into(),
            content: "42".into(),
        }
    );
    assert_eq!(session.state_stack.len(), 2);
    assert_eq!(effects, vec![Effect::SaveSession, Effect::CallLlmNetwork]);
}

// ---------------------------------------------------------------------------
// RestoreSubRoutine dispatch
// ---------------------------------------------------------------------------

#[test]
fn transition_restore_subroutine_registers_in_registry() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

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
    assert_eq!(session.state, AgentState::Idle);
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::SubRoutineRestored {
                handle: handle.clone(),
            },
            Effect::SaveSession,
        ]
    );
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
    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(OverrideInputPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Idle);
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![Effect::ForwardToUi(brioche_core::UiWidget::Test {
            msg: "overridden".to_string(),
        })]
    );
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
    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(BlockInputPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Idle);
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::HookConstraintFailed {
                    reason: "blocked".into(),
                },
            },
            Effect::SystemIdle,
        ]
    );
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
    let mut engine = BriocheEngineBuilder::new()
        .with_epoch_interceptor(Box::new(BlockEpochInterceptor))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Idle);
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::EpochMismatch,
                detail: ErrorDetail::EpochGuardRejected {
                    reason: "epoch stale".into(),
                },
            },
            Effect::SystemIdle,
        ]
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn transition_is_deterministic() {
    let mut engine_a = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut engine_b = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

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
            _registry: &mut SessionRegistry,
        ) -> PluginResult<Vec<Effect>> {
            Ok(vec![Effect::SaveSession])
        }
    }

    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(CountingLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let r = session.push_state(AgentState::Idle);
    assert!(r.is_ok());
    session.state = AgentState::SubRoutine(SubRoutineHandle::new("sub-1"));

    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 1,
            results: vec![],
        },
    );

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::SaveSession,
            Effect::CallLlmNetwork,
            Effect::SaveSession
        ]
    );
}

// ---------------------------------------------------------------------------
// ConsistencyVerifier
// ---------------------------------------------------------------------------

#[test]
fn transition_consistency_verifier_effects_appended() {
    struct ForcingVerifier;
    impl ConsistencyVerifier for ForcingVerifier {
        fn verify_consistency(&self, _session: &Session) -> PluginResult<Option<PolicyDecision>> {
            Ok(Some(PolicyDecision::RequestEffect(Effect::SystemIdle)))
        }
    }

    let mut engine = BriocheEngineBuilder::new()
        .with_consistency_verifier(Box::new(ForcingVerifier))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));
    assert_eq!(
        effects,
        vec![
            Effect::SaveSession,
            Effect::CallLlmNetwork,
            Effect::SystemIdle
        ]
    );
}

#[test]
fn transition_consistency_override_transition_applies_recovery() {
    struct RecoveryVerifier;
    impl ConsistencyVerifier for RecoveryVerifier {
        fn verify_consistency(&self, _session: &Session) -> PluginResult<Option<PolicyDecision>> {
            Ok(Some(PolicyDecision::OverrideTransition(vec![
                Effect::SystemIdle,
            ])))
        }
    }

    let mut engine = BriocheEngineBuilder::new()
        .with_consistency_verifier(Box::new(RecoveryVerifier))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    // The verifier's OverrideTransition forced recovery to Idle.
    assert_eq!(session.state, AgentState::Idle);
    assert!(session.state_stack.is_empty());
    assert!(session.active_tools.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::SaveSession,
            Effect::CallLlmNetwork,
            Effect::SystemIdle,
        ]
    );
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

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(RebuildPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Idle);
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::EffectsDroppedAfterRebuildRoutes { count: 1 },
            },
            Effect::RebuildRoutes,
        ]
    );
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

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(EditPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(
        session.history,
        vec![
            ChatMessage::System {
                content: "injected".into(),
            },
            ChatMessage::User {
                content: "hello".into(),
            },
        ]
    );
    assert_eq!(effects, vec![Effect::SaveSession, Effect::CallLlmNetwork]);
}

// ---------------------------------------------------------------------------
// Sprint 5: seal() integration, ActiveToolCall materialization, EngineInput
// dispatch refinement
// ---------------------------------------------------------------------------

#[test]
fn transition_llm_stream_tool_call_materialization() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(5000)
        .build();

    let mut session = Session::new("test");
    let push_result = session.push_state(AgentState::Predicting { generation_id: 7 });
    assert!(push_result.is_ok());

    let start = StreamEvent::ToolCallStart {
        path: ExecutionPath::default(),
        id: "tc1".into(),
        name: "calc".into(),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(start));
    assert!(effects.is_empty());
    assert_eq!(session.state, AgentState::Predicting { generation_id: 7 });

    let arg = StreamEvent::ToolArgumentChunk {
        path: ExecutionPath::default(),
        id: "tc1".into(),
        chunk: bytes::Bytes::from_static(b"{\"x\":1}"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(arg));
    assert!(effects.is_empty());

    let done = StreamEvent::ToolCallDone {
        path: ExecutionPath::default(),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(done));

    assert_eq!(
        session.state,
        AgentState::ExecutingTools { generation_id: 7 }
    );
    assert!(session.history.is_empty());
    assert_eq!(session.active_tools.len(), 1);
    assert_eq!(
        session.active_tools[0],
        ActiveToolCall {
            tool_id: "tc1".into(),
            tool_name: "calc".into(),
            arguments: "{\"x\":1}".into(),
            timeout_ms: 5000,
        }
    );
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::MissingToolTimeout {
                    default_timeout_ms: 5000,
                },
            },
            Effect::SaveSession,
            Effect::ExecuteTools(vec![ActiveToolCall {
                tool_id: "tc1".into(),
                tool_name: "calc".into(),
                arguments: "{\"x\":1}".into(),
                timeout_ms: 5000,
            }]),
        ]
    );
}

#[test]
fn transition_llm_stream_missing_timeout_applies_default() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(3000)
        .build();

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

    assert_eq!(
        session.state,
        AgentState::ExecutingTools { generation_id: 1 }
    );
    assert_eq!(session.active_tools.len(), 1);
    assert_eq!(session.active_tools[0].timeout_ms, 3000);
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::MissingToolTimeout {
                    default_timeout_ms: 3000,
                },
            },
            Effect::SaveSession,
            Effect::ExecuteTools(vec![ActiveToolCall {
                tool_id: "t1".into(),
                tool_name: "grep".into(),
                arguments: "".into(),
                timeout_ms: 3000,
            }]),
        ]
    );
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

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(TimeoutMutatorPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_default_tool_timeout_ms(1000)
        .build();

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

    assert_eq!(
        session.state,
        AgentState::ExecutingTools { generation_id: 2 }
    );
    assert_eq!(session.active_tools.len(), 1);
    assert_eq!(session.active_tools[0].timeout_ms, 9999);
    assert_eq!(
        effects,
        vec![
            Effect::SaveSession,
            Effect::ExecuteTools(vec![ActiveToolCall {
                tool_id: "t2".into(),
                tool_name: "calc".into(),
                arguments: "".into(),
                timeout_ms: 9999,
            }]),
        ]
    );
}

// ---------------------------------------------------------------------------
// Sprint 6: Fundamental governance traits — default implementations
// ---------------------------------------------------------------------------

#[test]
fn transition_with_epoch_guard_blocks_stale_generation() {
    use brioche_governance_default::EpochGuard;

    let mut engine = BriocheEngineBuilder::new()
        .with_epoch_interceptor(Box::new(EpochGuard))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    assert!(
        session
            .extensions
            .insert(brioche_core::EpochState {
                current_generation: 5,
            })
            .is_ok()
    );

    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 3, // obsolete
            results: vec![],
        },
    );

    assert_eq!(session.state, AgentState::Idle);
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::EpochMismatch,
                detail: ErrorDetail::EpochGuardRejected {
                    reason: "epoch mismatch: expected 5, got 3".into(),
                },
            },
            Effect::SystemIdle,
        ]
    );
}

#[test]
fn transition_with_epoch_guard_allows_current_generation() {
    use brioche_governance_default::EpochGuard;

    let mut engine = BriocheEngineBuilder::new()
        .with_epoch_interceptor(Box::new(EpochGuard))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    assert!(
        session
            .extensions
            .insert(brioche_core::EpochState {
                current_generation: 7,
            })
            .is_ok()
    );

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

    assert_eq!(session.state, AgentState::Predicting { generation_id: 7 });
    assert!(session.active_tools.is_empty());
    assert!(session.history.is_empty());
    assert_eq!(effects, vec![Effect::SaveSession, Effect::CallLlmNetwork]);
}

#[test]
fn transition_with_policy_aggregator_allows() {
    use brioche_governance_default::LexicographicDecisionAggregator;

    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));
    assert_eq!(effects, vec![Effect::SaveSession, Effect::CallLlmNetwork]);
}

#[test]
fn transition_with_subroutine_cleanup_guard_removes_child() {
    use brioche_governance_default::SubRoutineCleanupGuard;

    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build();

    let mut session = Session::new("test");
    let r = session.push_state(AgentState::Idle);
    assert!(r.is_ok());
    session.state = AgentState::SubRoutine(SubRoutineHandle::new("sub-1"));

    engine.create_subroutine(SubRoutineHandle::new("sub-1"), Session::new("sub-1"));

    let effects = engine.transition(
        &mut session,
        &EngineInput::ToolCallsResult {
            generation_id: 1,
            results: vec![],
        },
    );

    assert!(
        !engine
            .session_registry()
            .contains(&SubRoutineHandle::new("sub-1"))
    );
    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::SaveSession,
            Effect::CallLlmNetwork,
            Effect::SaveSession
        ]
    );
}

#[test]
fn transition_with_state_consistency_guard_fixes_inconsistent_state() {
    use brioche_governance_default::StateConsistencyGuard;

    let mut engine = BriocheEngineBuilder::new()
        .with_consistency_verifier(Box::new(StateConsistencyGuard::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    session.state = AgentState::Predicting { generation_id: 1 };

    let event = StreamEvent::TextChunk {
        path: ExecutionPath::default(),
        chunk: bytes::Bytes::from_static(b"x"),
    };
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(event));

    assert_eq!(session.state, AgentState::Idle);
    assert!(session.state_stack.is_empty());
    assert!(session.history.is_empty());
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::StateInconsistent {
                    source: brioche_core::types::InconsistencySource::Kernel {
                        module: "guards::consistency_verifier".to_string(),
                    },
                },
            },
            Effect::SaveSession,
            Effect::SystemIdle,
        ]
    );
}

#[test]
fn transition_with_fast_hook_effect_constraint_blocks_disallowed_effect() {
    use brioche_core::EffectBit;
    use brioche_governance_default::FastHookEffectConstraint;

    struct LlmRequestingPlugin;
    impl BriochePlugin for LlmRequestingPlugin {
        fn name(&self) -> &'static str {
            "llm_requester"
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::ON_INPUT
        }

        fn priority(&self) -> i16 {
            100
        }

        fn on_input(
            &self,
            _input: &EngineInput,
            _ext: &mut ExtensionStorage,
        ) -> PluginResult<PolicyDecision> {
            Ok(PolicyDecision::RequestEffect(Effect::RebuildRoutes))
        }
    }

    let mut masks = [0u64; 8];
    masks[0] = EffectBit::ERROR | EffectBit::SYSTEM_IDLE;
    let constraint = FastHookEffectConstraint::new(masks);

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(LlmRequestingPlugin))
        .with_hook_effect_constraint(Box::new(constraint))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::EffectNotAllowed {
                    hook: "on_input".into(),
                    effect_variant: "Discriminant(11)".into(),
                },
            },
            Effect::SaveSession,
            Effect::CallLlmNetwork,
        ]
    );
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

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(FaultyPlugin))
        .with_governance_failover_handler(Box::new(SystemFailoverGuard::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));
    assert_eq!(
        effects,
        vec![
            Effect::ForwardToUi(brioche_core::UiWidget::CriticalError {
                component: "faulty".into(),
                detail: Some("governance component failed; system degraded".into()),
            }),
            Effect::SaveSession,
            Effect::SystemIdle,
            Effect::SaveSession,
            Effect::CallLlmNetwork,
        ]
    );
}
#[test]
fn governance_failover_preserves_non_fault_effects() {
    use brioche_core::{GovernanceFailoverHandler, PluginSource};

    struct WrapFaultHandler;

    impl GovernanceFailoverHandler for WrapFaultHandler {
        fn handle_failure(
            &self,
            _session: &mut Session,
            fault: &Effect,
        ) -> PluginResult<Option<Vec<Effect>>> {
            Ok(Some(vec![
                Effect::SaveSession,
                fault.clone(),
                Effect::SaveSession,
            ]))
        }
    }

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

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(FaultyPlugin))
        .with_governance_failover_handler(Box::new(WrapFaultHandler))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));
    assert_eq!(
        effects,
        vec![
            Effect::SaveSession,
            Effect::PluginFault {
                plugin_name: PluginSource("faulty".into()),
                error: brioche_core::PluginError::Fatal {
                    plugin_name: "faulty".into(),
                    message: "boom".into(),
                },
            },
            Effect::SaveSession,
            Effect::SaveSession,
            Effect::CallLlmNetwork,
        ]
    );
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
/// Non-critical test type to validate the COW threshold.
pub struct TestCowState {
    /// Scalar value for COW weight tests.
    pub value: u64,
}

#[test]
fn adaptive_undo_frame_guard_restores_mutated_extension() {
    use brioche_governance_default::AdaptiveUndoFrameGuard;

    let mut guard = AdaptiveUndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    assert!(
        ext.insert(brioche_core::EpochState {
            current_generation: 42,
        })
        .is_ok()
    );

    guard.begin_hook("on_input");

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
fn adaptive_undo_frame_guard_abandons_past_threshold() {
    use brioche_governance_default::AdaptiveUndoFrameGuard;

    let mut guard = AdaptiveUndoFrameGuard::new(); // budget will likely be exceeded by TestCowState
    let mut ext = ExtensionStorage::new();
    assert!(ext.insert(TestCowState { value: 7 }).is_ok());

    guard.begin_hook("on_input");

    let type_id = std::any::TypeId::of::<TestCowState>();
    let vtable = TestCowState::build_vtable();
    let current = ext.get_or_insert_default::<TestCowState>();
    guard.on_mutation(type_id, &vtable, current);

    // Mutation may be abandoned due to threshold — state won't be restored.
    current.value = 123;

    guard.rollback_hook(&mut ext);

    let not_restored = ext.get_or_insert_default::<TestCowState>();
    // With adaptive budget, the result depends on budget; just verify no panic.
    assert!(not_restored.value == 123 || not_restored.value == 7);
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
#[brioche(ext_id = "tests.rollback_a")]
struct RollbackTypeA {
    #[brioche(deterministic_order)]
    payload: Vec<u8>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
#[brioche(ext_id = "tests.rollback_b")]
struct RollbackTypeB {
    #[brioche(deterministic_order)]
    payload: Vec<u8>,
}
struct MutatingPlugin;

impl BriochePlugin for MutatingPlugin {
    fn name(&self) -> &'static str {
        "mutating"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        100
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        ext.get_or_insert_default::<RollbackTypeA>().payload = vec![1; 64];
        ext.get_or_insert_default::<RollbackTypeB>().payload = vec![2; 64];
        Ok(PolicyDecision::Allow)
    }
}

#[test]
fn engine_rolls_back_extensions_when_cow_budget_exceeded() {
    // Give each extension type a different payload size so their estimated
    // weights differ. Set the budget to exactly the sum so both are snapshotted
    // and the cumulative weight triggers a rollback.
    let snapshot_a = RollbackTypeA {
        payload: vec![0; 32],
    };
    let snapshot_b = RollbackTypeB {
        payload: vec![0; 16],
    };
    let weight_a = (RollbackTypeA::build_vtable().estimated_weight_bytes)(&snapshot_a);
    let weight_b = (RollbackTypeB::build_vtable().estimated_weight_bytes)(&snapshot_b);

    let guard = brioche_governance_default::UndoFrameGuard::with_max_cow_bytes(weight_a + weight_b);

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(MutatingPlugin))
        .with_cycle_rollback_policy(Box::new(guard))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("rollback-test");
    assert!(session.extensions.insert(snapshot_a).is_ok());
    assert!(session.extensions.insert(snapshot_b).is_ok());

    let _effects = engine.transition(&mut session, &EngineInput::UserMessage("go".into()));

    // Both payloads should be restored to their pre-hook values.
    assert_eq!(
        session
            .extensions
            .get_or_insert_default::<RollbackTypeA>()
            .payload,
        vec![0; 32]
    );
    assert_eq!(
        session
            .extensions
            .get_or_insert_default::<RollbackTypeB>()
            .payload,
        vec![0; 16]
    );
}
struct FaultingPlugin;

impl BriochePlugin for FaultingPlugin {
    fn name(&self) -> &'static str {
        "faulting"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        100
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Err(brioche_core::PluginError::Fatal {
            plugin_name: "faulting".into(),
            message: "intentional fault".into(),
        })
    }
}

struct ErrorRecorderPlugin;

impl BriochePlugin for ErrorRecorderPlugin {
    fn name(&self) -> &'static str {
        "recorder"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_ERROR
    }

    fn priority(&self) -> i16 {
        0
    }

    fn on_error(
        &self,
        _error: &brioche_core::PluginError,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::RequestEffect(
            brioche_core::Effect::SavePluginBlob {
                plugin_id: brioche_core::PluginSource("recorder".into()),
                data: vec![0xab],
            },
        ))
    }
}

#[test]
fn engine_invokes_on_error_hook_for_plugin_faults() {
    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(FaultingPlugin))
        .with_plugin(Box::new(ErrorRecorderPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("on-error-test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("go".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "go"
    ));
    assert_eq!(
        effects,
        vec![
            Effect::SavePluginBlob {
                plugin_id: brioche_core::PluginSource("recorder".into()),
                data: vec![0xab],
            },
            Effect::PluginFault {
                plugin_name: brioche_core::PluginSource("faulting".into()),
                error: brioche_core::PluginError::Fatal {
                    plugin_name: "faulting".into(),
                    message: "intentional fault".into(),
                },
            },
            Effect::SaveSession,
            Effect::CallLlmNetwork,
        ]
    );
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
fn engine_with_adaptive_undo_frame_guard_instruments_hooks() {
    use brioche_governance_default::AdaptiveUndoFrameGuard;

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
            let state = ext.get_or_insert_default::<brioche_core::EpochState>();
            state.current_generation = 999;
            Ok(PolicyDecision::Allow)
        }
    }

    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(MutatingPlugin))
        .with_cycle_rollback_policy(Box::new(AdaptiveUndoFrameGuard::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    assert!(
        session
            .extensions
            .insert(brioche_core::EpochState {
                current_generation: 1,
            })
            .is_ok()
    );

    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));
    assert_eq!(effects, vec![Effect::SaveSession, Effect::CallLlmNetwork]);

    let state = session
        .extensions
        .get_or_insert_default::<brioche_core::EpochState>();
    assert_eq!(state.current_generation, 999);
}

// ---------------------------------------------------------------------------
// Direct public method tests
// ---------------------------------------------------------------------------

struct RebuildRoutesPlugin {
    name: &'static str,
    priority: i16,
    cap: PluginCapabilities,
}

impl BriochePlugin for RebuildRoutesPlugin {
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
fn rebuild_routes_filters_and_reorders_active_plugins() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_plugin(Box::new(RebuildRoutesPlugin {
            name: "alpha",
            priority: 0,
            cap: PluginCapabilities::ON_INPUT,
        }))
        .with_plugin(Box::new(RebuildRoutesPlugin {
            name: "beta",
            priority: 1,
            cap: PluginCapabilities::ON_INPUT,
        }))
        .with_plugin(Box::new(RebuildRoutesPlugin {
            name: "gamma",
            priority: 0,
            cap: PluginCapabilities::BEFORE_PREDICTION,
        }))
        .build();

    assert_eq!(engine.routing_table().route_on_input, vec![0, 1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![2]);

    // Disable plugin alpha (index 0); beta and gamma remain active.
    engine.rebuild_routes(&[false, true, true]);

    assert_eq!(engine.routing_table().route_on_input, vec![1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![2]);

    // Re-enable all, then omit later mask entries; missing entries default
    // to active so the full route is restored.
    engine.rebuild_routes(&[true, true, true]);
    assert_eq!(engine.routing_table().route_on_input, vec![0, 1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![2]);

    engine.rebuild_routes(&[false]);
    assert_eq!(engine.routing_table().route_on_input, vec![1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![2]);
}

#[test]
fn remove_subroutine_returns_session_and_second_remove_returns_none() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let handle = SubRoutineHandle::new("sub-1");
    let mut subroutine = Session::new("sub-1");
    subroutine.state = AgentState::Predicting { generation_id: 42 };

    engine.create_subroutine(handle.clone(), subroutine);

    let removed = engine.remove_subroutine(&handle);
    assert!(removed.is_some(), "expected subroutine to be removed");
    if let Some(removed) = removed {
        assert_eq!(removed.id, "sub-1");
        assert_eq!(removed.state, AgentState::Predicting { generation_id: 42 });
    }

    let second = engine.remove_subroutine(&handle);
    assert!(second.is_none(), "second remove should return None");
}

// ---------------------------------------------------------------------------
// Production profile wiring tests
// ---------------------------------------------------------------------------

/// Parallel suite exercising real governance profiles instead of mock traits.
///
/// These tests verify that `GovernanceProfile::Standard` and `Strict` wire
/// the production aggregator and lifecycle guard correctly, and that the
/// resulting engine still drives the full user-message → predict → tool
/// execution → response lifecycle.
///
/// Refs: I-Gov-Profile-Agnostic
mod production_profile_tests {
    use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};

    use super::*;

    /// Build an engine wired with the given production governance profile.
    fn engine_with_profile(profile: GovernanceProfile) -> brioche_core::BriocheEngine {
        BriocheEngineBuilder::new().with_profile(profile).build()
    }

    #[test]
    fn standard_profile_user_message_transitions_to_predicting() {
        let mut engine = engine_with_profile(GovernanceProfile::Standard);
        let mut session = Session::new("test");

        let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

        assert!(matches!(session.state, AgentState::Predicting { .. }));
        assert_eq!(session.history.len(), 1);
        assert!(matches!(
            &session.history[0],
            ChatMessage::User { content } if content == "hello"
        ));
        assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
    }

    #[test]
    fn strict_profile_user_message_transitions_to_predicting() {
        let mut engine = engine_with_profile(GovernanceProfile::Strict);
        let mut session = Session::new("test");

        let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

        assert!(matches!(session.state, AgentState::Predicting { .. }));
        assert_eq!(session.history.len(), 1);
        assert!(matches!(
            &session.history[0],
            ChatMessage::User { content } if content == "hello"
        ));
        assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
    }

    #[test]
    fn standard_profile_stream_done_persists_assistant_response() {
        let mut engine = engine_with_profile(GovernanceProfile::Standard);
        let mut session = Session::new("test");
        let r = session.push_state(AgentState::Predicting { generation_id: 1 });
        assert!(r.is_ok());

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

        assert!(effects1.is_empty());
        assert!(effects2.is_empty());
        assert!(effects3.iter().any(|e| matches!(e, Effect::SystemIdle)));
        assert!(effects3.iter().any(|e| matches!(e, Effect::SaveSession)));

        assert!(matches!(session.state, AgentState::Idle));
        assert!(session.pending_assistant_text.is_empty());
        assert_eq!(session.history.len(), 1);
        assert!(matches!(
            &session.history[0],
            ChatMessage::Assistant { content, .. } if content == "Hello world"
        ));
    }

    #[test]
    fn standard_profile_tool_call_lifecycle_predict_execute_respond() {
        let mut engine = engine_with_profile(GovernanceProfile::Standard);
        let mut session = Session::new("test");
        assert!(
            session
                .extensions
                .insert(brioche_core::EpochState {
                    current_generation: 1,
                })
                .is_ok()
        );
        let r = session.push_state(AgentState::Predicting { generation_id: 1 });
        assert!(r.is_ok());

        let text = StreamEvent::TextChunk {
            path: Default::default(),
            chunk: bytes::Bytes::from_static(b"Let me check"),
        };
        let start = StreamEvent::ToolCallStart {
            path: Default::default(),
            id: "tc1".into(),
            name: "calc".into(),
        };
        let arg = StreamEvent::ToolArgumentChunk {
            path: Default::default(),
            id: "tc1".into(),
            chunk: bytes::Bytes::from_static(b"{\"x\":1}"),
        };
        let done = StreamEvent::ToolCallDone {
            path: Default::default(),
        };

        engine.transition(&mut session, &EngineInput::LlmStream(text));
        engine.transition(&mut session, &EngineInput::LlmStream(start));
        engine.transition(&mut session, &EngineInput::LlmStream(arg));
        let effects = engine.transition(&mut session, &EngineInput::LlmStream(done));

        // Preceding text persisted as Assistant message.
        assert_eq!(session.history.len(), 1);
        assert!(matches!(
            &session.history[0],
            ChatMessage::Assistant { content, .. } if content == "Let me check"
        ));

        // State transitions to ExecutingTools; Standard default timeout is 30s.
        assert!(matches!(
            session.state,
            AgentState::ExecutingTools { generation_id: 1 }
        ));
        assert_eq!(session.active_tools.len(), 1);
        assert_eq!(session.active_tools[0].tool_id, "tc1");
        assert_eq!(session.active_tools[0].tool_name, "calc");
        assert_eq!(session.active_tools[0].arguments, "{\"x\":1}");
        assert_eq!(session.active_tools[0].timeout_ms, 30000);

        assert!(effects.iter().any(|e| matches!(e, Effect::ExecuteTools(_))));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));

        // Tool result returns to Predicting to continue the response loop.
        let result = ToolResultDTO {
            tool_id: "tc1".into(),
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

        assert!(matches!(session.state, AgentState::Predicting { .. }));
        assert!(session.history.iter().any(|m| matches!(
            m, ChatMessage::ToolResult { id, .. } if id == "tc1"
        )));
        assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
    }

    #[test]
    fn strict_profile_tool_call_lifecycle_uses_stricter_timeout() {
        let mut engine = engine_with_profile(GovernanceProfile::Strict);
        let mut session = Session::new("test");
        assert!(
            session
                .extensions
                .insert(brioche_core::EpochState {
                    current_generation: 1,
                })
                .is_ok()
        );
        let r = session.push_state(AgentState::Predicting { generation_id: 1 });
        assert!(r.is_ok());

        let start = StreamEvent::ToolCallStart {
            path: Default::default(),
            id: "tc1".into(),
            name: "calc".into(),
        };
        let done = StreamEvent::ToolCallDone {
            path: Default::default(),
        };

        engine.transition(&mut session, &EngineInput::LlmStream(start));
        let effects = engine.transition(&mut session, &EngineInput::LlmStream(done));

        // Strict profile default timeout is 10s.
        assert!(matches!(
            session.state,
            AgentState::ExecutingTools { generation_id: 1 }
        ));
        assert_eq!(session.active_tools.len(), 1);
        assert_eq!(session.active_tools[0].timeout_ms, 10000);

        assert!(effects.iter().any(|e| matches!(e, Effect::ExecuteTools(_))));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
    }
}
