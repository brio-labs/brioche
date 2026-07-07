//! Integration tests for governance interception and failover contracts.
//!
//! Refs: I-Gov-Decision-Isolation, I-Core-NoPanic, I-Core-RetVecEffect

use brioche_core::{
    AgentState, BriocheEngineBuilder, ChatMessage, ConsistencyVerifier, Effect, EngineInput,
    EpochAction, EpochInterceptor, ErrorCode, ErrorDetail, ExecutionPath, ExtensionStorage,
    OnInput, OnToolCalls, OnToolResult, PluginResult, PolicyDecision, Session, SessionRegistry,
    StreamEvent, SubRoutineHandle, SubRoutineLifecycleGuard, ToolCallDescriptor, ToolResultDTO,
};

mod common;
use common::{MockDecisionAggregator, MockSubRoutineLifecycleGuard};

struct BlockEpochInterceptor;

impl EpochInterceptor for BlockEpochInterceptor {
    type EngineInput = EngineInput;
    type EpochAction = EpochAction;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

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

#[test]
fn transition_exits_subroutine_triggers_lifecycle_guard() {
    struct CountingLifecycleGuard;
    impl SubRoutineLifecycleGuard for CountingLifecycleGuard {
        type Effect = Effect;
        type PluginError = brioche_core::PluginError;
        type Session = Session;
        type SessionRegistry = SessionRegistry;
        type SubRoutineHandle = SubRoutineHandle;

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
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;
        type Session = Session;

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
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;
        type Session = Session;

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
fn transition_with_subroutine_timeout_intercepts_before_child_delegation() {
    use brioche_governance_default::{
        EpochGuard, SubRoutineOrchestrator, SubRoutineTimeoutPolicy, SubRoutineTimerState,
    };

    let mut engine = BriocheEngineBuilder::new()
        .with_epoch_interceptor(Box::new(EpochGuard))
        .with_epoch_interceptor(Box::new(SubRoutineTimeoutPolicy::new()))
        .with_subroutine_handler(Box::new(SubRoutineOrchestrator::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let handle = SubRoutineHandle::new("sub-1");
    let mut session = Session::new("parent");
    let pushed = session.push_state(AgentState::SubRoutine(handle.clone()));
    assert!(pushed.is_ok());

    {
        let buffer = session
            .extensions
            .get_or_insert_default::<brioche_core::SignalBuffer>();
        buffer
            .system_signals
            .push(brioche_core::SystemSignal::Tick { elapsed_ms: 101 });

        let state = session
            .extensions
            .get_or_insert_default::<SubRoutineTimerState>();
        state.timers.insert(handle.clone(), (0, 100));
    }

    engine.create_subroutine(handle.clone(), Session::new("child"));

    let effects = engine.transition(&mut session, &EngineInput::UserMessage("continue".into()));

    assert_eq!(session.state, AgentState::SubRoutine(handle.clone()));
    assert!(engine.session_registry().contains(&handle));
    assert_eq!(
        effects,
        vec![
            Effect::Error {
                code: ErrorCode::EpochMismatch,
                detail: ErrorDetail::EpochGuardRejected {
                    reason: "sub-routine SubRoutineHandle(\"sub-1\") exceeded timeout".into(),
                },
            },
            Effect::SystemIdle,
        ]
    );
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
    impl OnInput for LlmRequestingPlugin {
        type EngineInput = EngineInput;
        type ExtensionStorage = ExtensionStorage;
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;

        fn name(&self) -> &'static str {
            "llm_requester"
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
        .with_on_input(Box::new(LlmRequestingPlugin))
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
    impl OnInput for FaultyPlugin {
        type EngineInput = EngineInput;
        type ExtensionStorage = ExtensionStorage;
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;

        fn name(&self) -> &'static str {
            "faulty"
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
        .with_on_input(Box::new(FaultyPlugin))
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
        type Effect = Effect;
        type PluginError = brioche_core::PluginError;
        type Session = Session;

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
    impl OnInput for FaultyPlugin {
        type EngineInput = EngineInput;
        type ExtensionStorage = ExtensionStorage;
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;

        fn name(&self) -> &'static str {
            "faulty"
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
        .with_on_input(Box::new(FaultyPlugin))
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
