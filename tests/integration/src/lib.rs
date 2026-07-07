//! Cross-crate integration smoke tests.
//!
//! Verifies that `brioche-core`, `brioche-governance-default`, and
//! `brioche-shell-runtime` wire together correctly end-to-end.
//!
//! Refs: I-Core-NoPanic, I-Gov-Profile-Agnostic, I-Shell-TransitionJournal

#![cfg(test)]

use brioche_core::{BriocheEngineBuilder, EngineInput, Session};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
use brioche_shell_runtime::transition_journal::{JournalEntry, TransitionJournal};

/// Build an engine using the standard governance profile.
fn build_standard_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Standard)
        .build()
}

/// User messages routed through a standard-profile engine produce effects.
#[test]
fn engine_governance_profile_routes_user_message() {
    let mut engine = build_standard_engine();
    let mut session = Session::new("integration_smoke");

    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(!effects.is_empty(), "user message should produce effects");
}

/// The transition journal records and replays user messages without loss.
#[test]
fn transition_journal_records_and_replays_user_messages() {
    let journal = TransitionJournal::new();
    let inputs = [
        EngineInput::UserMessage("first".into()),
        EngineInput::UserMessage("second".into()),
    ];

    for input in &inputs {
        journal.append(input);
    }

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    for (entry, original) in entries.iter().zip(inputs.iter()) {
        assert_eq!(entry, &JournalEntry::Input(original.clone()));
    }
}

/// The async shell accepts input when wired with a standard-profile engine.
#[tokio::test]
async fn shell_runtime_with_default_profile_accepts_input() {
    use brioche_shell_runtime::{
        BriocheShell, DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence,
        ShellConfig,
    };

    let shell = BriocheShell::new(
        || (build_standard_engine(), Session::new("integration_smoke")),
        ShellConfig::default(),
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence),
        None,
    );

    let result = shell
        .send_input(EngineInput::UserMessage("async smoke".into()))
        .await;

    assert!(result.is_ok(), "shell should accept input without error");
}
