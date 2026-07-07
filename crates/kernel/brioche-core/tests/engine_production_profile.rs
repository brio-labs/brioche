//! Integration tests for production governance profile wiring.
//!
//! Refs: I-Gov-Profile-Agnostic

use brioche_core::{
    AgentState, BriocheEngineBuilder, ChatMessage, Effect, EngineInput, Session, StreamEvent,
    ToolResultDTO,
};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};

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
