//! Integration tests for routing, plugin-order, and hook-decision contracts.
//!
//! Refs: I-Core-StreamNoBranch, I-Core-PluginOrder, I-Core-RetVecEffect

use brioche_core::{
    AgentState, BeforePrediction, BriocheEngineBuilder, ChatMessage, Effect, EngineInput,
    ErrorCode, ErrorDetail, ExtensionStorage, HistoryEdit, OnInput, OnInputPlugin, OnStreamEvent,
    OnStreamEventPlugin, PluginResult, PolicyDecision, Session, StreamEvent, SubRoutineHandle,
    UnifiedRoutingTable,
};

mod common;
use common::{MockDecisionAggregator, MockSubRoutineLifecycleGuard};

struct PriorityTestPlugin {
    name: &'static str,
    priority: i16,
}

impl OnInput for PriorityTestPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> i16 {
        self.priority
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

impl OnStreamEvent for PriorityTestPlugin {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type StreamAction = brioche_core::StreamAction;
    type StreamEvent = StreamEvent;

    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> i16 {
        self.priority
    }

    fn on_stream_event(
        &self,
        _event: &StreamEvent,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<brioche_core::StreamAction> {
        Ok(brioche_core::StreamAction::Pass)
    }
}

#[test]
fn routing_table_orders_by_priority_then_name() {
    let on_input: Vec<Box<OnInputPlugin>> = vec![
        Box::new(PriorityTestPlugin {
            name: "beta",
            priority: 0,
        }),
        Box::new(PriorityTestPlugin {
            name: "alpha",
            priority: 0,
        }),
        Box::new(PriorityTestPlugin {
            name: "gamma",
            priority: -1,
        }),
    ];

    let table = UnifiedRoutingTable::from_hooks(&on_input, &[], &[], &[], &[], &[], &[]);

    // Expected order: gamma (-1), alpha (0, "alpha" < "beta"), beta (0).
    assert_eq!(table.route_on_input, vec![2, 1, 0]);
}

#[test]
fn routing_table_filters_by_capability() {
    let on_input: Vec<Box<OnInputPlugin>> = vec![Box::new(PriorityTestPlugin {
        name: "input_only",
        priority: 0,
    })];
    let on_stream_event: Vec<Box<OnStreamEventPlugin>> = vec![Box::new(PriorityTestPlugin {
        name: "stream_only",
        priority: 0,
    })];

    let table =
        UnifiedRoutingTable::from_hooks(&on_input, &[], &on_stream_event, &[], &[], &[], &[]);

    assert_eq!(table.route_on_input, vec![0]);
    assert_eq!(table.route_on_stream_event, vec![0]);
    assert!(table.route_before_prediction.is_empty());
}

// On-input policy decision contracts

struct OverrideInputPlugin;

impl OnInput for OverrideInputPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "override_input"
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
        .with_on_input(Box::new(OverrideInputPlugin))
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

impl OnInput for BlockInputPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "block_input"
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
        .with_on_input(Box::new(BlockInputPlugin))
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

#[test]
fn transition_rebuildroutes_is_last() {
    struct RebuildPlugin;
    impl OnInput for RebuildPlugin {
        type EngineInput = EngineInput;
        type ExtensionStorage = ExtensionStorage;
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;

        fn name(&self) -> &'static str {
            "rebuild"
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
        .with_on_input(Box::new(RebuildPlugin))
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

// History mutation hook contract

#[test]
fn transition_history_edit_insert_and_truncate() {
    struct EditPlugin;
    impl BeforePrediction for EditPlugin {
        type ChatMessage = ChatMessage;
        type ExtensionStorage = ExtensionStorage;
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;

        fn name(&self) -> &'static str {
            "edit"
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
        .with_before_prediction(Box::new(EditPlugin))
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

struct RebuildRoutesPlugin {
    name: &'static str,
    priority: i16,
}

impl OnInput for RebuildRoutesPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> i16 {
        self.priority
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

struct RebuildRoutesBeforePredictionPlugin {
    name: &'static str,
    priority: i16,
}

impl BeforePrediction for RebuildRoutesBeforePredictionPlugin {
    type ChatMessage = ChatMessage;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        self.name
    }

    fn priority(&self) -> i16 {
        self.priority
    }

    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

#[test]
fn rebuild_routes_filters_and_reorders_active_plugins() {
    let mut engine = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_on_input(Box::new(RebuildRoutesPlugin {
            name: "alpha",
            priority: 0,
        }))
        .with_on_input(Box::new(RebuildRoutesPlugin {
            name: "beta",
            priority: 1,
        }))
        .with_before_prediction(Box::new(RebuildRoutesBeforePredictionPlugin {
            name: "gamma",
            priority: 0,
        }))
        .build();

    assert_eq!(engine.routing_table().route_on_input, vec![0, 1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![0]);

    // Disable plugin alpha (index 0); beta and gamma remain active.
    engine.rebuild_routes(&[false, true, true]);

    assert_eq!(engine.routing_table().route_on_input, vec![1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![0]);

    // Re-enable all, then omit later mask entries; missing entries default
    // to active so the full route is restored.
    engine.rebuild_routes(&[true, true, true]);
    assert_eq!(engine.routing_table().route_on_input, vec![0, 1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![0]);

    engine.rebuild_routes(&[false]);
    assert_eq!(engine.routing_table().route_on_input, vec![1]);
    assert_eq!(engine.routing_table().route_before_prediction, vec![0]);
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
