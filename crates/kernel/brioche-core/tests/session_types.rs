//! Book I — Sprint 3 integration tests: Session, AgentState, SessionRegistry,
//! SessionSnapshot, `seal()`, and `HistoryEdit`.
//!
//! Invariants verified:
//! - I-Core-Pure: deterministic construction and transitions.
//! - I-Core-NoPanic: invalid transitions produce `BriocheError`, never panics.
//! - I-Core-ActiveToolCall: `seal()` maps `ToolCallDescriptor` exhaustively.
//! - I-Gov-Decision-Isolation: `HistoryEdit` indices are validated before mutation.

use brioche_core::types::HistoryOperation;
use brioche_core::{
    AgentState, AgentStateTag, BriocheError, ChatMessage, DEFAULT_TOOL_TIMEOUT_MS,
    ExtensionStorage, HistoryEdit, Session, SessionRegistry, SessionSnapshot, SubRoutineHandle,
    ToolCallDescriptor, seal,
};

// ---------------------------------------------------------------------------
// Session construction
// ---------------------------------------------------------------------------

#[test]
fn session_new_starts_idle() {
    let session = Session::new("test-1");
    assert_eq!(session.id, "test-1");
    assert!(matches!(session.state, AgentState::Idle));
    assert!(session.history.is_empty());
    assert_eq!(session.persisted_msg_count, 0);
    assert!(session.state_stack.is_empty());
    assert!(session.active_tools.is_empty());
}

#[test]
fn session_default_is_idle() {
    let session = Session::default();
    assert!(matches!(session.state, AgentState::Idle));
    assert_eq!(session.id, "");
}

// ---------------------------------------------------------------------------
// State stack
// ---------------------------------------------------------------------------

#[test]
fn push_state_transitions_and_stacks() {
    let mut session = Session::new("s");
    let result = session.push_state(AgentState::Predicting { generation_id: 1 });
    assert!(result.is_ok());
    assert!(matches!(
        session.state,
        AgentState::Predicting { generation_id: 1 }
    ));
    assert_eq!(session.state_stack.len(), 1);
    assert!(matches!(session.state_stack[0], AgentState::Idle));
}

#[test]
fn pop_state_restores_previous() {
    let mut session = Session::new("s");
    let push = session.push_state(AgentState::Predicting { generation_id: 2 });
    assert!(push.is_ok());
    let popped = session.pop_state();
    assert!(popped.is_ok());
    if let Ok(popped_state) = popped {
        assert!(matches!(
            popped_state,
            AgentState::Predicting { generation_id: 2 }
        ));
    } else {
        assert_eq!(1, 0, "pop_state should succeed");
    }
    assert!(matches!(session.state, AgentState::Idle));
    assert!(session.state_stack.is_empty());
}

#[test]
fn push_failure_is_rejected() {
    let mut session = Session::new("s");
    let result = session.push_state(AgentState::Failure);
    assert!(
        matches!(result, Err(BriocheError::InvalidStateTransition(ref msg)) if msg.contains("Failure"))
    );
}

#[test]
fn pop_empty_stack_is_rejected() {
    let mut session = Session::new("s");
    let result = session.pop_state();
    assert!(
        matches!(result, Err(BriocheError::InvalidStateTransition(ref msg)) if msg.contains("empty"))
    );
}

// ---------------------------------------------------------------------------
// SessionRegistry
// ---------------------------------------------------------------------------

#[test]
fn registry_insert_and_get_mut() {
    let mut registry = SessionRegistry::new();
    let handle = SubRoutineHandle::new("sub-1");
    let session = Session::new("sub-1");
    registry.insert(handle.clone(), session);
    assert!(registry.contains(&handle));
    assert!(registry.get_mut(&handle).is_some());
}

#[test]
fn registry_remove_returns_session() {
    let mut registry = SessionRegistry::new();
    let handle = SubRoutineHandle::new("sub-2");
    registry.insert(handle.clone(), Session::new("sub-2"));
    let removed = registry.remove(&handle);
    assert!(removed.is_some());
    assert!(!registry.contains(&handle));
}

#[test]
fn registry_remove_unknown_returns_none() {
    let mut registry = SessionRegistry::new();
    let handle = SubRoutineHandle::new("sub-3");
    assert!(registry.remove(&handle).is_none());
}

#[test]
fn registry_exit_count_increments() {
    let mut registry = SessionRegistry::new();
    let handle = SubRoutineHandle::new("sub-4");
    assert_eq!(registry.get_exit_count(&handle), 0);
    registry.increment_exit_count(&handle);
    assert_eq!(registry.get_exit_count(&handle), 1);
    registry.increment_exit_count(&handle);
    assert_eq!(registry.get_exit_count(&handle), 2);
}

#[test]
fn registry_handles_iterates_keys() {
    let mut registry = SessionRegistry::new();
    let h1 = SubRoutineHandle::new("a");
    let h2 = SubRoutineHandle::new("b");
    registry.insert(h1.clone(), Session::new("a"));
    registry.insert(h2.clone(), Session::new("b"));
    let keys: Vec<_> = registry.handles().cloned().collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&h1));
    assert!(keys.contains(&h2));
}

// ---------------------------------------------------------------------------
// seal()
// ---------------------------------------------------------------------------

#[test]
fn seal_maps_descriptor_to_active() {
    let descriptors = vec![ToolCallDescriptor {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        arguments: "{\"x\":1}".into(),
        timeout_ms: Some(5000),
    }];
    let active = seal(descriptors, DEFAULT_TOOL_TIMEOUT_MS);
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].tool_id, "t1");
    assert_eq!(active[0].tool_name, "calc");
    assert_eq!(active[0].arguments, "{\"x\":1}");
    assert_eq!(active[0].timeout_ms, 5000);
}

#[test]
fn seal_none_timeout_defaults_to_kernel_default() {
    let descriptors = vec![ToolCallDescriptor {
        tool_id: "t2".into(),
        tool_name: "grep".into(),
        arguments: "pattern".into(),
        timeout_ms: None,
    }];
    let active = seal(descriptors, DEFAULT_TOOL_TIMEOUT_MS);
    assert_eq!(active[0].timeout_ms, DEFAULT_TOOL_TIMEOUT_MS);
}

#[test]
fn seal_empty_vec() {
    let active = seal(vec![], DEFAULT_TOOL_TIMEOUT_MS);
    assert!(active.is_empty());
}

// ---------------------------------------------------------------------------
// SessionSnapshot
// ---------------------------------------------------------------------------

#[test]
fn snapshot_reflects_current_state() {
    let mut session = Session::new("s");
    assert_eq!(session.snapshot().current_state, AgentStateTag::Idle);
    assert_eq!(session.snapshot().state_stack_depth, 0);

    assert!(
        session
            .push_state(AgentState::Predicting { generation_id: 7 })
            .is_ok()
    );
    assert_eq!(session.snapshot().current_state, AgentStateTag::Predicting);
    assert_eq!(session.snapshot().state_stack_depth, 1);
}

#[test]
fn snapshot_as_extension_type_roundtrip() {
    let mut storage = ExtensionStorage::new();
    let snap = SessionSnapshot {
        current_state: AgentStateTag::ExecutingTools,
        state_stack_depth: 3,
    };
    let result = storage.insert(snap.clone());
    assert!(result.is_ok(), "SessionSnapshot serializes: {result:?}");

    let retrieved = storage.get_mut::<SessionSnapshot>();
    if let Some(snap) = retrieved {
        assert_eq!(snap.current_state, AgentStateTag::ExecutingTools);
        assert_eq!(snap.state_stack_depth, 3);
    } else {
        assert_eq!(1, 0, "SessionSnapshot should be in storage");
    }
}

#[test]
fn agent_state_tag_from_subroutine() {
    let state = AgentState::SubRoutine(SubRoutineHandle::new("x"));
    assert_eq!(AgentStateTag::from(&state), AgentStateTag::SubRoutine);
}

// ---------------------------------------------------------------------------
// Determinism sanity
// ---------------------------------------------------------------------------

#[test]
fn identical_sessions_produce_identical_snapshots() {
    let s1 = Session::new("det");
    let s2 = Session::new("det");
    assert_eq!(s1.snapshot(), s2.snapshot());
}

#[test]
fn deterministic_state_machine_sequence() {
    let mut session = Session::new("seq");
    assert!(
        session
            .push_state(AgentState::Predicting { generation_id: 1 })
            .is_ok()
    );
    assert!(
        session
            .push_state(AgentState::ExecutingTools { generation_id: 1 })
            .is_ok()
    );
    assert!(session.pop_state().is_ok());
    assert!(session.pop_state().is_ok());
    assert!(matches!(session.state, AgentState::Idle));
    assert!(session.state_stack.is_empty());
}

// ---------------------------------------------------------------------------
// History edits
// ---------------------------------------------------------------------------

#[test]
fn history_replace_valid() {
    let mut session = Session::new("s");
    let first = ChatMessage::User {
        content: "first".into(),
    };
    let second = ChatMessage::User {
        content: "second".into(),
    };
    let replacement = ChatMessage::User {
        content: "replaced".into(),
    };

    session.history.push(first.clone());
    session.history.push(second.clone());

    let result = session.apply_history_edits(&[HistoryEdit::Replace {
        index: 1,
        message: replacement.clone(),
    }]);
    assert!(result.is_ok());
    assert_eq!(session.history.len(), 2);
    assert_eq!(session.history[0], first);
    assert_eq!(session.history[1], replacement);
}

#[test]
fn history_replace_out_of_bounds() {
    let mut session = Session::new("s");
    session.history.push(ChatMessage::User {
        content: "only".into(),
    });

    let result = session.apply_history_edits(&[HistoryEdit::Replace {
        index: 1,
        message: ChatMessage::User {
            content: "x".into(),
        },
    }]);
    assert!(matches!(
        result,
        Err(BriocheError::HistoryIndexOutOfBounds {
            operation: HistoryOperation::Replace,
            index: 1,
            len: 1,
        })
    ));
}

#[test]
fn history_replace_empty_history() {
    let mut session = Session::new("s");

    let result = session.apply_history_edits(&[HistoryEdit::Replace {
        index: 0,
        message: ChatMessage::User {
            content: "x".into(),
        },
    }]);
    assert!(matches!(
        result,
        Err(BriocheError::HistoryIndexOutOfBounds {
            operation: HistoryOperation::Replace,
            index: 0,
            len: 0,
        })
    ));
}

#[test]
fn history_truncate_valid() {
    let mut session = Session::new("s");
    session.history.push(ChatMessage::User {
        content: "a".into(),
    });
    session.history.push(ChatMessage::User {
        content: "b".into(),
    });
    session.history.push(ChatMessage::User {
        content: "c".into(),
    });

    let result = session.apply_history_edits(&[HistoryEdit::Truncate { keep_last: 2 }]);
    assert!(result.is_ok());
    assert_eq!(session.history.len(), 2);
    assert_eq!(
        session.history[0],
        ChatMessage::User {
            content: "b".into(),
        }
    );
    assert_eq!(
        session.history[1],
        ChatMessage::User {
            content: "c".into(),
        }
    );
}

#[test]
fn history_truncate_keep_zero() {
    let mut session = Session::new("s");
    session.history.push(ChatMessage::User {
        content: "a".into(),
    });

    let result = session.apply_history_edits(&[HistoryEdit::Truncate { keep_last: 0 }]);
    assert!(result.is_ok());
    assert!(session.history.is_empty());
}

#[test]
fn history_truncate_out_of_bounds_is_lenient() {
    let mut session = Session::new("s");
    session.history.push(ChatMessage::User {
        content: "a".into(),
    });
    session.history.push(ChatMessage::User {
        content: "b".into(),
    });

    let result = session.apply_history_edits(&[HistoryEdit::Truncate { keep_last: 10 }]);
    assert!(result.is_ok());
    assert_eq!(session.history.len(), 2);
}

#[test]
fn history_truncate_empty_history() {
    let mut session = Session::new("s");

    let result = session.apply_history_edits(&[HistoryEdit::Truncate { keep_last: 5 }]);
    assert!(result.is_ok());
    assert!(session.history.is_empty());
}

#[test]
fn history_insert_out_of_bounds_returns_error() {
    let mut session = Session::new("s");
    session.history.push(ChatMessage::User {
        content: "a".into(),
    });

    let result = session.apply_history_edits(&[HistoryEdit::Insert {
        index: 2,
        message: ChatMessage::User {
            content: "x".into(),
        },
    }]);
    assert!(matches!(
        result,
        Err(BriocheError::HistoryIndexOutOfBounds {
            operation: HistoryOperation::Insert,
            index: 2,
            len: 1,
        })
    ));
}

#[test]
fn history_empty_edits_is_no_op() {
    let mut session = Session::new("s");
    session.history.push(ChatMessage::User {
        content: "a".into(),
    });

    let result = session.apply_history_edits(&[]);
    assert!(result.is_ok());
    assert_eq!(session.history.len(), 1);
}

#[test]
fn history_multiple_edits_apply_sequentially() {
    let mut session = Session::new("s");
    session.history.push(ChatMessage::User {
        content: "a".into(),
    });
    session.history.push(ChatMessage::User {
        content: "b".into(),
    });

    let result = session.apply_history_edits(&[
        HistoryEdit::Replace {
            index: 0,
            message: ChatMessage::User {
                content: "a2".into(),
            },
        },
        HistoryEdit::Truncate { keep_last: 1 },
    ]);
    assert!(result.is_ok());
    assert_eq!(session.history.len(), 1);
    assert_eq!(
        session.history[0],
        ChatMessage::User {
            content: "b".into(),
        }
    );
}
