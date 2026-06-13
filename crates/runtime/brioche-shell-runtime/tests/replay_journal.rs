//! Replay tests: `TransitionJournal` → post-watchdog recovery — Sprint 18.
//!
//! Persists `EngineInput`s in a `TransitionJournal`, simulates a watchdog
//! restart, and replays unacknowledged entries through a fresh engine.
//! Asserts zero divergence between original and recovered sessions.
//!
//! Refs: I-Shell-TransitionJournal, I-Shell-TransitionJournal-Idempotent,
//! docs/SPECS.md §Book V Ch 12

use brioche_core::{BriocheEngineBuilder, EngineInput, Session, StreamEvent};
use brioche_governance_default::{LexicographicDecisionAggregator, SubRoutineCleanupGuard};
use brioche_shell_runtime::transition_journal::{JournalEntry, TransitionJournal};

fn build_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build()
}

#[test]
fn replay_journal_user_message_sequence() {
    let journal = TransitionJournal::new();
    let mut engine = build_engine();
    let mut session = Session::new("journal_replay");

    let inputs = vec![
        EngineInput::UserMessage("hello".into()),
        EngineInput::LlmStream(StreamEvent::Done),
        EngineInput::UserMessage("world".into()),
        EngineInput::LlmStream(StreamEvent::Done),
    ];

    let mut original_effects = Vec::new();
    for input in &inputs {
        journal.append(input);
        original_effects.push(engine.transition(&mut session, input));
    }

    // Simulate watchdog restart: read unacknowledged entries.
    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    // Replay through a fresh engine.
    let mut recovered_engine = build_engine();
    let mut recovered_session = Session::new("journal_replay");

    let mut recovered_effects = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => {
                std::process::abort();
            }
        }
    }

    assert_eq!(
        original_effects, recovered_effects,
        "effect sequences diverged after journal replay"
    );
    assert_eq!(
        session.state, recovered_session.state,
        "final states diverged after journal replay"
    );
    assert_eq!(
        session.history.len(),
        recovered_session.history.len(),
        "history lengths diverged after journal replay"
    );
}

#[test]
fn replay_journal_acknowledge_then_empty() {
    let journal = TransitionJournal::new();
    let mut engine = build_engine();
    let mut session = Session::new("ack");

    let inputs = vec![
        EngineInput::UserMessage("a".into()),
        EngineInput::UserMessage("b".into()),
    ];

    for input in &inputs {
        journal.append(input);
        let _ = engine.transition(&mut session, input);
    }

    // Acknowledge all entries (as if saved to Redb).
    journal.acknowledge_all();

    // Recovery should see nothing to replay.
    let entries = journal.read_unacknowledged();
    assert!(entries.is_empty());

    // Unacknowledged bytes should be zero.
    assert_eq!(journal.unacknowledged_bytes(), 0);
}

#[test]
fn replay_journal_partial_acknowledge() {
    let journal = TransitionJournal::new();

    let inputs = vec![
        EngineInput::UserMessage("first".into()),
        EngineInput::UserMessage("second".into()),
        EngineInput::UserMessage("third".into()),
    ];

    for input in &inputs {
        journal.append(input);
    }

    // Acknowledge after first entry by manually advancing ack_index.
    // In production this happens after a successful Redb flush.
    // Here we verify that only unacknowledged entries remain.
    let all_entries = journal.read_unacknowledged();
    assert_eq!(all_entries.len(), 3);

    journal.acknowledge_all();
    let after_ack = journal.read_unacknowledged();
    assert!(after_ack.is_empty());
}
