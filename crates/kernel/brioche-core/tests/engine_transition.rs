//! Integration tests for core transition state-machine dispatch.
//!
//! Refs: I-Core-NoPanic, I-Core-RetVecEffect, I-Core-Pure

use brioche_core::{
    ActiveToolCall, AgentState, BriocheEngineBuilder, ChatMessage, Effect, EngineInput, ErrorCode,
    ErrorDetail, Session, StreamEvent, SubRoutineHandle, ToolResultDTO,
};

mod common;
use common::{MockDecisionAggregator, MockSubRoutineLifecycleGuard};

// User-message transition contract

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

// LLM stream transition contract

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

// Tool-call result transition contract

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

// Sub-routine restore transition contract

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
