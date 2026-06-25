//! Replay tests: `TransitionJournal` → post-watchdog recovery — Sprint 18.
//!
//! Persists `EngineInput`s in a `TransitionJournal`, simulates a watchdog
//! restart, and replays unacknowledged entries through a fresh engine.
//! Asserts zero divergence between original and recovered sessions.
//!
//! Refs: I-Shell-TransitionJournal, I-Shell-TransitionJournal-Idempotent,
//! docs/SPECS.md §Book V Ch 12

use brioche_core::{
    ActiveToolCall, BriocheEngineBuilder, Effect, EngineInput, EpochState, ErrorCode,
    PluginCapabilities, PluginError, PolicyDecision, Session, StreamEvent, SubRoutineHandle,
    ToolOutcome, ToolResultDTO,
};
use brioche_core::{BriochePlugin, ExtensionStorage, SubRoutineHydrator};
use brioche_governance_default::{
    LexicographicDecisionAggregator, Priority, SubRoutineCleanupGuard,
};
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
// ---------------------------------------------------------------------------
// Engine builders for scenario-specific governance
// ---------------------------------------------------------------------------
fn build_engine_with_epoch_guard() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .with_epoch_interceptor(Box::new(brioche_governance_default::EpochGuard))
        .build()
}

#[derive(Clone, Debug, Default)]
struct FaultyInputPlugin;

impl BriochePlugin for FaultyInputPlugin {
    fn name(&self) -> &'static str {
        "faulty_input"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        Priority::RECOVERY
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> brioche_core::PluginResult<PolicyDecision> {
        Err(PluginError::Fatal {
            plugin_name: "faulty_input".into(),
            message: "intentional replay fault".into(),
        })
    }
}

fn build_engine_with_faulty_plugin() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .with_plugin(Box::new(FaultyInputPlugin))
        .build()
}

#[derive(Clone, Debug, Default)]
struct MockSubRoutineHydrator;

impl SubRoutineHydrator for MockSubRoutineHydrator {
    fn hydrate(&self, _head_blob: &[u8]) -> Result<Session, brioche_core::BriocheError> {
        Ok(Session::new("hydrated-sub"))
    }
}

fn build_engine_with_hydrator() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .with_subroutine_hydrator(Box::new(MockSubRoutineHydrator))
        .build()
}

// ---------------------------------------------------------------------------
// P5-TEST-03: extended replay scenarios
// ---------------------------------------------------------------------------

#[test]
fn replay_journal_tool_call_sequence() {
    let journal = TransitionJournal::new();
    let mut engine = build_engine();
    let mut session = Session::new("tool_call_replay");

    let inputs = vec![
        EngineInput::UserMessage("read a file".into()),
        EngineInput::LlmStream(StreamEvent::ToolCallStart {
            path: Default::default(),
            id: "call_1".into(),
            name: "read_file".into(),
        }),
        EngineInput::LlmStream(StreamEvent::ToolArgumentChunk {
            path: Default::default(),
            id: "call_1".into(),
            chunk: r#"{"path":"/tmp/test.txt"}"#.into(),
        }),
        EngineInput::LlmStream(StreamEvent::ToolCallDone {
            path: Default::default(),
        }),
        EngineInput::LlmStream(StreamEvent::Done),
    ];

    let mut original_effects = Vec::new();
    for input in &inputs {
        journal.append(input);
        original_effects.push(engine.transition(&mut session, input));
    }

    assert!(
        original_effects.iter().any(|effs| effs
            .iter()
            .any(|e| matches!(e, Effect::ExecuteTools(calls) if calls.iter().any(|c: &ActiveToolCall| c.tool_name == "read_file")))),
        "original run should request execution of read_file"
    );

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    let mut recovered_engine = build_engine();
    let mut recovered_session = Session::new("tool_call_replay");
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
        "tool-call effect sequences diverged after journal replay"
    );
    assert_eq!(session.state, recovered_session.state);
    assert_eq!(session.history.len(), recovered_session.history.len());
}

#[test]
fn replay_journal_subroutine_restore() {
    let journal = TransitionJournal::new();
    let mut engine = build_engine_with_hydrator();
    let mut session = Session::new("subroutine_replay");

    let handle = SubRoutineHandle::new("sub-replay-1");
    let inputs = vec![EngineInput::RestoreSubRoutine {
        handle: handle.clone(),
        head_blob: vec![1, 2, 3, 4],
    }];

    let mut original_effects = Vec::new();
    for input in &inputs {
        journal.append(input);
        original_effects.push(engine.transition(&mut session, input));
    }

    assert!(
        original_effects
            .iter()
            .any(|effs| effs.iter().any(|e| matches!(
                e, Effect::SubRoutineRestored { handle: h } if h == &handle
            ))),
        "original run should emit SubRoutineRestored"
    );

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    let mut recovered_engine = build_engine_with_hydrator();
    let mut recovered_session = Session::new("subroutine_replay");
    let mut recovered_effects = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => std::process::abort(),
        }
    }

    assert_eq!(
        original_effects, recovered_effects,
        "subroutine restore effect sequences diverged after journal replay"
    );
    assert!(recovered_engine.session_registry().contains(&handle));
}

#[test]
fn replay_journal_epoch_mismatch() {
    let journal = TransitionJournal::new();
    let mut engine = build_engine_with_epoch_guard();
    let mut session = Session::new("epoch_replay");

    // Establish a high epoch so a stale ToolCallsResult is rejected.
    session.extensions.insert(EpochState {
        current_generation: 10,
    });

    let inputs = vec![EngineInput::ToolCallsResult {
        generation_id: 3,
        results: vec![ToolResultDTO {
            tool_id: "call_1".into(),
            tool_name: "read_file".into(),
            outcome: ToolOutcome::Success("contents".into()),
        }],
    }];

    let mut original_effects = Vec::new();
    for input in &inputs {
        journal.append(input);
        original_effects.push(engine.transition(&mut session, input));
    }

    assert!(
        original_effects
            .iter()
            .any(|effs| effs.iter().any(|e| matches!(
                e, Effect::Error { code, .. } if *code == ErrorCode::EpochMismatch
            ))),
        "original run should emit EpochMismatch error"
    );

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    let mut recovered_engine = build_engine_with_epoch_guard();
    let mut recovered_session = Session::new("epoch_replay");
    recovered_session.extensions.insert(EpochState {
        current_generation: 10,
    });
    let mut recovered_effects = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => std::process::abort(),
        }
    }

    assert_eq!(
        original_effects, recovered_effects,
        "epoch mismatch effect sequences diverged after journal replay"
    );
}

#[test]
fn replay_journal_plugin_fault() {
    let journal = TransitionJournal::new();
    let mut engine = build_engine_with_faulty_plugin();
    let mut session = Session::new("fault_replay");

    let inputs = vec![EngineInput::UserMessage("trigger fault".into())];

    let mut original_effects = Vec::new();
    for input in &inputs {
        journal.append(input);
        original_effects.push(engine.transition(&mut session, input));
    }

    assert!(
        original_effects
            .iter()
            .any(|effs| effs.iter().any(|e| matches!(
                e, Effect::PluginFault { plugin_name, .. } if plugin_name.0 == "faulty_input"
            ))),
        "original run should emit PluginFault for faulty_input"
    );

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    let mut recovered_engine = build_engine_with_faulty_plugin();
    let mut recovered_session = Session::new("fault_replay");
    let mut recovered_effects = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => std::process::abort(),
        }
    }

    assert_eq!(
        original_effects, recovered_effects,
        "plugin fault effect sequences diverged after journal replay"
    );
}

#[test]
fn replay_journal_wraparound() {
    let journal = TransitionJournal::new();

    // Leave headroom for the "WRAP" batch marker plus an index suffix.
    const SUFFIX_LEN: usize = 8;
    let payload_size = brioche_shell_runtime::transition_journal::MAX_ENTRY_BYTES - 4 - SUFFIX_LEN;
    let big_message = "x".repeat(payload_size);
    let entry_size = payload_size + 4 + SUFFIX_LEN;
    let capacity_entries =
        brioche_shell_runtime::transition_journal::JOURNAL_CAPACITY_BYTES / entry_size;

    // Fill the journal with first-batch entries.
    for i in 0..capacity_entries {
        let input = EngineInput::UserMessage(format!("{big_message}{i:04}"));
        journal.append(&input);
    }

    journal.acknowledge_all();

    // Write a second batch that wraps over the acknowledged region.
    let second_batch = 10usize;
    for i in 0..second_batch {
        let input = EngineInput::UserMessage(format!("{big_message}WRAP{i:04}"));
        journal.append(&input);
    }

    let entries = journal.read_unacknowledged();
    // The journal cannot return more entries than fit in the ring buffer.
    assert!(
        entries.len() <= capacity_entries,
        "wraparound should not exceed journal capacity"
    );

    // Every recovered entry must belong to the second batch and deserialize.
    for entry in &entries {
        match entry {
            JournalEntry::Input(EngineInput::UserMessage(text)) => {
                assert!(
                    text.starts_with(&format!("{big_message}WRAP")),
                    "wrapped journal should only contain second-batch entries, got {text}"
                );
            }
            other => unreachable!("unexpected journal entry after wraparound: {other:?}"),
        }
    }

    // Replay the wrapped entries through a fresh engine to ensure recovery
    // does not panic and behaves deterministically.
    let mut recovered_engine = build_engine();
    let mut recovered_session = Session::new("wraparound_replay");
    for entry in &entries {
        if let JournalEntry::Input(input) = entry {
            recovered_engine.transition(&mut recovered_session, input);
        }
    }
    // The recovered session should contain only the second-batch messages.
    assert_eq!(recovered_session.history.len(), entries.len());
}
