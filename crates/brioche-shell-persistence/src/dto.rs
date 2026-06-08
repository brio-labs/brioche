//! Session Data Transfer Objects (DTOs) for persistence.
//!
//! `SessionHeadDTO` is the MessagePack-serializable representation of a
//! `Session` used by the Redb persistence layer. It flattens the
//! hierarchical automaton state and strips runtime-only pointers.
//!
//! Refs: SPECS.md §Book III-B Ch 1.1, I-Persist-Idempotence

use brioche_core::{AgentState, Session};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Versioned schema for forward-compatible session head serialization.
///
/// Read-Upgrade-Write: a `V1` blob loaded from disk is upgraded to `V2`
/// in memory before the next write.
///
/// Refs: SPECS.md §Book III-B Ch 1.1
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionSchemaVersion {
    /// Initial schema (Sprint 12).
    V1 = 1,
    // V2 reserved for future schema evolution.
}

/// Flattened, serializable representation of `AgentState`.
///
/// `SubRoutine` stores only the opaque child handle string, eliminating
/// the live `Session` reference. `Failure` is a terminal variant with no
/// payload.
///
/// Refs: I-Persist-Idempotence, I-Core-AgentState
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlattenedAgentState {
    /// Waiting for user input.
    Idle,
    /// LLM prediction in progress.
    Predicting { generation_id: u64 },
    /// Tools are being executed by the shell.
    ExecutingTools { generation_id: u64 },
    /// Delegated to a sub-routine; stores the opaque handle only.
    SubRoutine(String),
    /// Terminal failure state.
    Failure,
}

impl From<&AgentState> for FlattenedAgentState {
    fn from(state: &AgentState) -> Self {
        match state {
            AgentState::Idle => Self::Idle,
            AgentState::Predicting { generation_id } => Self::Predicting {
                generation_id: *generation_id,
            },
            AgentState::ExecutingTools { generation_id } => Self::ExecutingTools {
                generation_id: *generation_id,
            },
            AgentState::SubRoutine(handle) => Self::SubRoutine(handle.as_str().to_string()),
            AgentState::Failure => Self::Failure,
            _ => Self::Idle,
        }
    }
}

impl TryFrom<FlattenedAgentState> for AgentState {
    type Error = brioche_core::BriocheError;

    fn try_from(state: FlattenedAgentState) -> Result<Self, Self::Error> {
        match state {
            FlattenedAgentState::Idle => Ok(Self::Idle),
            FlattenedAgentState::Predicting { generation_id } => {
                Ok(Self::Predicting { generation_id })
            }
            FlattenedAgentState::ExecutingTools { generation_id } => {
                Ok(Self::ExecutingTools { generation_id })
            }
            FlattenedAgentState::SubRoutine(handle) => Ok(Self::SubRoutine(
                brioche_core::SubRoutineHandle::new(handle)?,
            )),
            FlattenedAgentState::Failure => Ok(Self::Failure),
        }
    }
}

/// Session head DTO — the unit of atomic persistence.
///
/// Contains everything needed to reconstruct a `Session` except the
/// message history, which is stored separately in `MESSAGES_TABLE`.
///
/// # Invariants
/// - `extensions` contains only the `cold_snapshot` blobs from
///   `ExtensionStorage`, never hot-map pointers.
/// - `state_stack` is fully flattened; no live `Session` references.
///
/// Refs: SPECS.md §Book III-B Ch 1.1, I-Persist-SaveSession
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHeadDTO {
    /// Schema version for migration support.
    pub version: SessionSchemaVersion,
    /// Session identifier (primary key in `SESSIONS_TABLE`).
    pub id: String,
    /// Parent session ID for tree reconstruction (`None` for root).
    pub parent_id: Option<String>,
    /// Current mechanical state (flattened).
    pub state: FlattenedAgentState,
    /// Hierarchical state stack (flattened).
    pub state_stack: Vec<FlattenedAgentState>,
    /// Extension cold snapshots: `ext_id` -> binary blob.
    pub extensions: BTreeMap<String, Vec<u8>>,
    /// Number of messages already persisted (delta protocol watermark).
    pub persisted_msg_count: usize,
    /// Opportunistic GC watermark (Sprint 13).
    pub compaction_index: u32,
}

impl SessionHeadDTO {
    /// Convert a live `Session` into its persistent DTO.
    ///
    /// Must be called on the engine thread before the `Session` is sent
    /// to the async runtime, because `Session` is `!Send`.
    ///
    /// Complexity: O(n) where n = `state_stack.len() + extensions.count()`.
    /// Allocates one `BTreeMap` and one `Vec`.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn from_session(session: &Session) -> Self {
        Self {
            version: SessionSchemaVersion::V1,
            id: session.id().to_string(),
            parent_id: None,
            state: FlattenedAgentState::from(session.state()),
            state_stack: session
                .state_stack()
                .iter()
                .map(FlattenedAgentState::from)
                .collect(),
            extensions: session
                .extensions()
                .cold_snapshot()
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            persisted_msg_count: session.persisted_msg_count(),
            compaction_index: 0,
        }
    }

    /// Reconstruct a live `Session` from the DTO and a message history.
    ///
    /// The session is created in memory and can be passed to
    /// `BriocheEngine::transition()` directly.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if the DTO contains
    /// an invalid sub-routine handle.
    ///
    /// Refs: I-Persist-Idempotence
    pub fn to_session(
        &self,
        history: Vec<brioche_core::ChatMessage>,
    ) -> Result<brioche_core::Session, brioche_core::BriocheError> {
        use brioche_core::Session;
        let mut session = Session::new(&self.id);
        session.set_history(history);
        session.set_persisted_msg_count(self.persisted_msg_count);
        session.restore_state(
            self.state.clone().try_into()?,
            self.state_stack
                .iter()
                .cloned()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
        );
        Ok(session)
    }
}
