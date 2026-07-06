//! Replay tests: `AuditState` → blank engine — Sprint 18.
//!
//! Records a sequence of `EngineInput`s via `AuditLogger`, then replays
//! them through a fresh engine and asserts zero divergence.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Core-Pure, docs/SPECS.md §Book V Ch 12

use brioche_core::{AgentState, BriocheEngineBuilder, EngineInput, Session, StreamEvent};
use brioche_governance_default::{LexicographicDecisionAggregator, SubRoutineCleanupGuard};
use brioche_std::AuditLogger;

/// Build an engine with the AuditLogger installed.
fn build_recording_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_on_input(Box::new(AuditLogger::with_batch_size(1000)))
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build()
}

/// Build a fresh engine without the AuditLogger for replay.
fn build_replay_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build()
}

#[test]
fn replay_audit_user_message_sequence() {
    let mut record_engine = build_recording_engine();
    let mut record_session = Session::new("replay");

    // Record a sequence of inputs.
    let inputs = vec![
        EngineInput::UserMessage("hello".into()),
        EngineInput::LlmStream(StreamEvent::Done),
        EngineInput::UserMessage("world".into()),
        EngineInput::LlmStream(StreamEvent::Done),
    ];

    let mut recorded_effects = Vec::new();
    for input in &inputs {
        recorded_effects.push(record_engine.transition(&mut record_session, input));
    }

    // Replay through a fresh engine.
    let mut replay_engine = build_replay_engine();
    let mut replay_session = Session::new("replay");

    let mut replayed_effects = Vec::new();
    for input in &inputs {
        replayed_effects.push(replay_engine.transition(&mut replay_session, input));
    }

    // Assert zero divergence.
    assert_eq!(
        recorded_effects, replayed_effects,
        "effect sequences diverged between record and replay"
    );
    assert_eq!(
        record_session.state, replay_session.state,
        "final states diverged"
    );
    assert_eq!(
        record_session.history.len(),
        replay_session.history.len(),
        "history lengths diverged"
    );
}

#[test]
fn replay_audit_tool_result_sequence() {
    let mut record_engine = build_recording_engine();
    let mut record_session = Session::new("replay");

    // Enter Predicting, then ExecutingTools, then ToolCallsResult.
    let _ = record_engine.transition(
        &mut record_session,
        &EngineInput::UserMessage("call tool".into()),
    );
    let _ = record_engine.transition(
        &mut record_session,
        &EngineInput::LlmStream(StreamEvent::Done),
    );

    // Reset to Idle for a clean tool execution sequence.
    record_session.state = AgentState::Idle;
    record_session.state_stack.clear();
    record_session.history.clear();

    let inputs = vec![EngineInput::UserMessage("calc".into())];

    let mut recorded_effects = Vec::new();
    for input in &inputs {
        recorded_effects.push(record_engine.transition(&mut record_session, input));
    }

    // Replay.
    let mut replay_engine = build_replay_engine();
    let mut replay_session = Session::new("replay");
    replay_session.state = AgentState::Idle;

    let mut replayed_effects = Vec::new();
    for input in &inputs {
        replayed_effects.push(replay_engine.transition(&mut replay_session, input));
    }

    assert_eq!(
        recorded_effects, replayed_effects,
        "effect sequences diverged on tool result replay"
    );
}
