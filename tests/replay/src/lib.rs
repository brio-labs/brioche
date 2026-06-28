//! Cross-crate replay tests.
//!
//! Records `EngineInput`s through the shell runtime transition journal,
//! simulates recovery, and replays through a fresh engine to assert zero
//! divergence.
//!
//! Refs: I-Shell-TransitionJournal, I-Shell-TransitionJournal-Idempotent

#![cfg(test)]

use brioche_core::{BriocheEngineBuilder, EngineInput, Session};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
use brioche_shell_runtime::transition_journal::{JournalEntry, TransitionJournal};

/// Build a permissive-profile engine for replay tests.
fn build_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Permissive)
        .build()
}

/// Replaying a recorded journal through a fresh engine yields identical effects.
#[test]
fn journal_replay_produces_identical_effects() {
    let journal = TransitionJournal::new();
    let mut original_engine = build_engine();
    let mut original_session = Session::new("replay_test");

    let inputs = [
        EngineInput::UserMessage("hello".into()),
        EngineInput::UserMessage("world".into()),
    ];

    let mut original_effects = Vec::new();
    for input in &inputs {
        journal.append(input);
        original_effects.push(original_engine.transition(&mut original_session, input));
    }

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    let mut replayed_engine = build_engine();
    let mut replayed_session = Session::new("replay_test");
    let mut replayed_effects = Vec::new();

    for entry in &entries {
        if let JournalEntry::Input(input) = entry {
            replayed_effects.push(replayed_engine.transition(&mut replayed_session, input));
        }
    }

    assert_eq!(original_effects, replayed_effects);
}

/// Acknowledging a journal clears unacknowledged entries.
#[test]
fn journal_acknowledge_clears_unacknowledged_entries() {
    let journal = TransitionJournal::new();
    journal.append(&EngineInput::UserMessage("ack me".into()));

    assert!(!journal.read_unacknowledged().is_empty());

    journal.acknowledge_all();

    assert!(journal.read_unacknowledged().is_empty());
}
