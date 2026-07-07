//! Session-head DTO conversion contracts.

use brioche_core::{AgentState, Session};
use brioche_shell_persistence::{FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion};

#[test]
fn session_head_dto_from_idle_session() {
    let session = Session::new("test-1");
    let dto = SessionHeadDTO::from_session(&session);

    assert_eq!(dto.id, "test-1");
    assert_eq!(dto.version, SessionSchemaVersion::V1);
    assert!(matches!(dto.state, FlattenedAgentState::Idle));
    assert!(dto.state_stack.is_empty());
    assert_eq!(dto.persisted_msg_count, 0);
    assert_eq!(dto.compaction_index, 0);
}

#[test]
fn session_head_dto_flattened_state_stack() {
    let mut session = Session::new("test-2");
    match session.push_state(AgentState::Predicting { generation_id: 7 }) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    match session.push_state(AgentState::ExecutingTools { generation_id: 7 }) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let dto = SessionHeadDTO::from_session(&session);

    assert_eq!(dto.state_stack.len(), 2);
    assert!(matches!(dto.state_stack[0], FlattenedAgentState::Idle));
    assert!(matches!(
        dto.state_stack[1],
        FlattenedAgentState::Predicting { generation_id: 7 }
    ));
    assert!(matches!(
        dto.state,
        FlattenedAgentState::ExecutingTools { generation_id: 7 }
    ));
}

#[test]
fn session_head_dto_subroutine_handle() {
    let mut session = Session::new("test-3");
    session.state = AgentState::SubRoutine(brioche_core::SubRoutineHandle::new("child-42"));

    let dto = SessionHeadDTO::from_session(&session);

    assert!(matches!(
        dto.state,
        FlattenedAgentState::SubRoutine(ref s) if s == "child-42"
    ));
}
