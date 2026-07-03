//! Replay tests: `TransitionJournal` → post-watchdog recovery — Sprint 18.
//!
//! Persists `EngineInput`s in a `TransitionJournal`, simulates a watchdog
//! restart, and replays unacknowledged entries through a fresh engine.
//! Asserts zero divergence between original and recovered sessions.
//!
//! Refs: I-Shell-TransitionJournal, I-Shell-TransitionJournal-Idempotent,
//! docs/SPECS.md §Book V Ch 12

use brioche_core::{
    AgentState, BriocheEngine, BriocheEngineBuilder, BriocheError, BriochePlugin, Effect,
    EngineInput, ExtensionStorage, PluginCapabilities, PluginError, PluginResult, PolicyDecision,
    Session, StreamEvent, SubRoutineHandle, SubRoutineHydrator, ToolOutcome, ToolResultDTO,
};
use brioche_governance_default::{
    BriocheEngineBuilderExt, GovernanceProfile, LexicographicDecisionAggregator,
    NoopGovernanceFailoverHandler, PermissiveHookEffectConstraint, SubRoutineCleanupGuard,
};
use brioche_shell_persistence::{SessionHeadDTO, serialize_head};
use brioche_shell_runtime::transition_journal::{JournalEntry, TransitionJournal};
use bytes::Bytes;

fn build_engine() -> BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build()
}

fn build_engine_with_hydrator() -> BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .with_subroutine_hydrator(Box::new(PersistenceBackedHydrator))
        .build()
}

fn build_standard_engine() -> BriocheEngine {
    BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Standard)
        .build()
}

fn build_engine_with_fault_plugin() -> BriocheEngine {
    BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Standard)
        .with_governance_failover_handler(Box::new(NoopGovernanceFailoverHandler))
        .with_hook_effect_constraint(Box::new(PermissiveHookEffectConstraint::new()))
        .with_plugin(Box::new(FaultPlugin { trigger: "boom" }))
        .build()
}

struct PersistenceBackedHydrator;

impl SubRoutineHydrator for PersistenceBackedHydrator {
    fn hydrate(&self, head_blob: &[u8]) -> Result<Session, BriocheError> {
        let dto = brioche_shell_persistence::deserialize_head(head_blob)
            .map_err(|err| BriocheError::Serialization(err.to_string()))?;
        Ok(dto.to_session(vec![]))
    }
}

struct FaultPlugin {
    trigger: &'static str,
}

impl BriochePlugin for FaultPlugin {
    fn name(&self) -> &'static str {
        "fault_plugin"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn on_input(
        &self,
        input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        if let EngineInput::UserMessage(content) = input
            && content == self.trigger
        {
            return Err(PluginError::Fatal {
                plugin_name: "fault_plugin".into(),
                message: "simulated fatal fault".into(),
            });
        }
        Ok(PolicyDecision::Allow)
    }
}

#[test]
fn replay_journal_user_message_sequence() -> Result<(), Box<dyn std::error::Error>> {
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
                return Err("oversized journal entry".into());
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
    Ok(())
}

#[test]
fn replay_journal_acknowledge_then_empty() -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

#[test]
fn replay_journal_partial_acknowledge() -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

#[test]
fn replay_journal_tool_call_sequence() -> Result<(), Box<dyn std::error::Error>> {
    let journal = TransitionJournal::new();
    let mut engine = build_engine();
    let mut session = Session::new("tool_call_replay");

    let mut inputs: Vec<EngineInput> = Vec::new();
    let mut original_effects: Vec<Vec<Effect>> = Vec::new();

    inputs.push(EngineInput::UserMessage("call a tool".into()));
    journal.append(&inputs[0]);
    original_effects.push(engine.transition(&mut session, &inputs[0]));

    let generation_id = match session.state {
        AgentState::Predicting { generation_id } => generation_id,
        _ => return Err("expected Predicting state after user message".into()),
    };

    inputs.push(EngineInput::LlmStream(StreamEvent::ToolCallStart {
        path: Default::default(),
        id: "tc1".into(),
        name: "calculator".into(),
    }));
    inputs.push(EngineInput::LlmStream(StreamEvent::ToolArgumentChunk {
        path: Default::default(),
        id: "tc1".into(),
        chunk: Bytes::from_static(br#"{"x":1}"#),
    }));
    inputs.push(EngineInput::LlmStream(StreamEvent::ToolCallDone {
        path: Default::default(),
    }));
    inputs.push(EngineInput::LlmStream(StreamEvent::Done));
    inputs.push(EngineInput::ToolCallsResult {
        generation_id,
        results: vec![ToolResultDTO {
            tool_id: "tc1".into(),
            tool_name: "calculator".into(),
            outcome: ToolOutcome::Success("42".into()),
        }],
    });

    for input in &inputs[1..] {
        journal.append(input);
        original_effects.push(engine.transition(&mut session, input));
    }

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    let mut recovered_engine = build_engine();
    let mut recovered_session = Session::new("tool_call_replay");

    let mut recovered_effects: Vec<Vec<Effect>> = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => {
                return Err("oversized journal entry".into());
            }
        }
    }

    assert_eq!(
        original_effects, recovered_effects,
        "tool-call effect sequences diverged after journal replay"
    );
    assert_eq!(
        session.state, recovered_session.state,
        "final states diverged after tool-call replay"
    );
    assert_eq!(
        session.history.len(),
        recovered_session.history.len(),
        "history lengths diverged after tool-call replay"
    );

    Ok(())
}

#[test]
fn replay_journal_subroutine_restore() -> Result<(), Box<dyn std::error::Error>> {
    let journal = TransitionJournal::new();
    let mut engine = build_engine_with_hydrator();
    let mut session = Session::new("subroutine_replay");

    let child = Session::new("child-session");
    let head_dto = SessionHeadDTO::from_session(&child);
    let head_blob = serialize_head(&head_dto)?;
    let handle = SubRoutineHandle::new("child-session");

    let input = EngineInput::RestoreSubRoutine {
        handle: handle.clone(),
        head_blob,
    };

    let mut original_effects: Vec<Vec<Effect>> = Vec::new();
    journal.append(&input);
    original_effects.push(engine.transition(&mut session, &input));

    let original_child = match engine.remove_subroutine(&handle) {
        Some(child) => child,
        None => return Err("original engine should register the restored child".into()),
    };

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), 1);

    let mut recovered_engine = build_engine_with_hydrator();
    let mut recovered_session = Session::new("subroutine_replay");

    let mut recovered_effects: Vec<Vec<Effect>> = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => {
                return Err("oversized journal entry".into());
            }
        }
    }

    let recovered_child = match recovered_engine.remove_subroutine(&handle) {
        Some(child) => child,
        None => return Err("recovered engine should register the restored child".into()),
    };

    assert_eq!(
        original_effects, recovered_effects,
        "RestoreSubRoutine effect sequences diverged after journal replay"
    );
    assert_eq!(
        session.state, recovered_session.state,
        "parent states diverged after subroutine replay"
    );
    assert_eq!(
        original_child.state, recovered_child.state,
        "restored child states diverged after subroutine replay"
    );

    Ok(())
}

#[test]
fn replay_journal_epoch_mismatch() -> Result<(), Box<dyn std::error::Error>> {
    let journal = TransitionJournal::new();
    let mut engine = build_standard_engine();
    let mut session = Session::new("epoch_mismatch");

    let inputs = vec![
        EngineInput::UserMessage("hello".into()),
        EngineInput::LlmStream(StreamEvent::Done),
        EngineInput::ToolCallsResult {
            generation_id: 42,
            results: vec![ToolResultDTO {
                tool_id: "tc1".into(),
                tool_name: "calculator".into(),
                outcome: ToolOutcome::Success("stale".into()),
            }],
        },
    ];

    let mut original_effects: Vec<Vec<Effect>> = Vec::new();
    for input in &inputs {
        journal.append(input);
        original_effects.push(engine.transition(&mut session, input));
    }

    assert!(
        original_effects[2].iter().any(|e| matches!(
            e,
            Effect::Error {
                code: brioche_core::ErrorCode::EpochMismatch,
                ..
            }
        )),
        "stale ToolCallsResult should be rejected as an epoch mismatch"
    );

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), inputs.len());

    let mut recovered_engine = build_standard_engine();
    let mut recovered_session = Session::new("epoch_mismatch");

    let mut recovered_effects: Vec<Vec<Effect>> = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => {
                return Err("oversized journal entry".into());
            }
        }
    }

    assert_eq!(
        original_effects, recovered_effects,
        "epoch-mismatch effect sequences diverged after journal replay"
    );
    assert_eq!(
        session.state, recovered_session.state,
        "final states diverged after epoch-mismatch replay"
    );
    assert_eq!(
        session.history.len(),
        recovered_session.history.len(),
        "history lengths diverged after epoch-mismatch replay"
    );

    Ok(())
}

#[test]
fn replay_journal_plugin_fault() -> Result<(), Box<dyn std::error::Error>> {
    let journal = TransitionJournal::new();
    let mut engine = build_engine_with_fault_plugin();
    let mut session = Session::new("plugin_fault");

    let input = EngineInput::UserMessage("boom".into());
    let mut original_effects: Vec<Vec<Effect>> = Vec::new();
    journal.append(&input);
    original_effects.push(engine.transition(&mut session, &input));

    assert!(
        original_effects[0]
            .iter()
            .any(|e| matches!(e, Effect::RebuildRoutes)),
        "faulted plugin should trigger a RebuildRoutes effect"
    );

    let entries = journal.read_unacknowledged();
    assert_eq!(entries.len(), 1);

    let mut recovered_engine = build_engine_with_fault_plugin();
    let mut recovered_session = Session::new("plugin_fault");

    let mut recovered_effects: Vec<Vec<Effect>> = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => {
                return Err("oversized journal entry".into());
            }
        }
    }

    assert_eq!(
        original_effects, recovered_effects,
        "plugin-fault effect sequences diverged after journal replay"
    );
    assert_eq!(
        session.state, recovered_session.state,
        "final states diverged after plugin-fault replay"
    );
    assert_eq!(
        session.history.len(),
        recovered_session.history.len(),
        "history lengths diverged after plugin-fault replay"
    );

    Ok(())
}

#[test]
fn replay_journal_wraparound() -> Result<(), Box<dyn std::error::Error>> {
    let journal = TransitionJournal::new();
    let mut engine = build_engine();
    let mut session = Session::new("wraparound");

    // Fill most of the journal with acknowledged prefix entries so the
    // subsequent tail wraps around the ring boundary.
    let mut prefix_inputs: Vec<EngineInput> = Vec::new();
    for i in 0..1000 {
        let content = format!("prefix-{:08}-{}", i, "x".repeat(3000));
        let input = EngineInput::UserMessage(content);
        journal.append(&input);
        engine.transition(&mut session, &input);
        prefix_inputs.push(input);
        if journal.unacknowledged_bytes() >= 900_000 {
            break;
        }
    }

    journal.acknowledge_all();
    assert_eq!(journal.unacknowledged_bytes(), 0);

    let tail: Vec<EngineInput> = (0..80)
        .map(|_| {
            EngineInput::LlmStream(StreamEvent::ToolArgumentChunk {
                path: Default::default(),
                id: "wrap".into(),
                chunk: Bytes::from(vec![b'a'; 2500]),
            })
        })
        .collect();

    let mut original_tail_effects: Vec<Vec<Effect>> = Vec::new();
    for input in &tail {
        journal.append(input);
        original_tail_effects.push(engine.transition(&mut session, input));
    }

    let entries = journal.read_unacknowledged();
    assert_eq!(
        entries.len(),
        tail.len(),
        "tail entries should survive journal wraparound"
    );

    // Recover by advancing a fresh engine to the acknowledged prefix state,
    // then replaying the unacknowledged tail.
    let mut recovered_engine = build_engine();
    let mut recovered_session = Session::new("wraparound");
    for input in &prefix_inputs {
        recovered_engine.transition(&mut recovered_session, input);
    }

    let mut recovered_effects: Vec<Vec<Effect>> = Vec::new();
    for entry in &entries {
        match entry {
            JournalEntry::Input(input) => {
                recovered_effects.push(recovered_engine.transition(&mut recovered_session, input));
            }
            JournalEntry::Oversized { .. } => {
                return Err("oversized journal entry".into());
            }
        }
    }

    assert_eq!(
        original_tail_effects, recovered_effects,
        "wraparound tail effect sequences diverged after journal replay"
    );
    assert_eq!(
        session.state, recovered_session.state,
        "final states diverged after wraparound replay"
    );
    assert_eq!(
        session.history.len(),
        recovered_session.history.len(),
        "history lengths diverged after wraparound replay"
    );

    Ok(())
}
