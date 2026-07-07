//! Session Data Transfer Objects (DTOs) for persistence.
//!
//! `SessionHeadDTO` is the MessagePack-serializable representation of a
//! `Session` used by the Redb persistence layer. It flattens the
//! hierarchical automaton state and strips runtime-only pointers.
//!
//! Refs: docs/SPECS.md §Book III-B Ch 1.1, I-Persist-Idempotence

use std::collections::BTreeMap;

use brioche_core::{AgentState, Session};
use serde::{Deserialize, Serialize};

/// Versioned schema for forward-compatible session head serialization.
///
/// Read-Upgrade-Write: a `V1` blob loaded from disk is upgraded to `V2`
/// in memory before the next write.
///
/// Refs: docs/SPECS.md §Book III-B Ch 1.1
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
    Predicting {
        /// Generation ID correlating async responses.
        generation_id: u64,
    },
    /// Tools are being executed by the shell.
    ExecutingTools {
        /// Generation ID matching the triggering prediction.
        generation_id: u64,
    },
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

impl From<FlattenedAgentState> for AgentState {
    fn from(state: FlattenedAgentState) -> Self {
        match state {
            FlattenedAgentState::Idle => Self::Idle,
            FlattenedAgentState::Predicting { generation_id } => Self::Predicting { generation_id },
            FlattenedAgentState::ExecutingTools { generation_id } => {
                Self::ExecutingTools { generation_id }
            }
            FlattenedAgentState::SubRoutine(handle) => {
                Self::SubRoutine(brioche_core::SubRoutineHandle::new(handle))
            }
            FlattenedAgentState::Failure => Self::Failure,
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
/// Refs: docs/SPECS.md §Book III-B Ch 1.1, I-Persist-SaveSession
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
    pub persisted_msg_count: u64,
    /// Opportunistic GC watermark (Sprint 13).
    pub compaction_index: u32,
    /// CRC32 checksum of the serialized DTO (excluding this field).
    ///
    /// `None` for legacy blobs; validation is skipped when absent.
    pub checksum: Option<u32>,
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
            id: session.id.clone(),
            parent_id: None,
            state: FlattenedAgentState::from(&session.state),
            state_stack: session
                .state_stack
                .iter()
                .map(FlattenedAgentState::from)
                .collect(),
            extensions: session.extensions.cold_snapshot().clone(),
            persisted_msg_count: session.persisted_msg_count,
            compaction_index: 0,
            checksum: None,
        }
    }

    /// Reconstruct a live `Session` from the DTO and a message history.
    ///
    /// The session is created in memory and can be passed to
    /// `BriocheEngine::transition()` directly.
    ///
    /// Refs: I-Persist-Idempotence
    pub fn to_session(&self, history: Vec<brioche_core::ChatMessage>) -> brioche_core::Session {
        use brioche_core::Session;
        let mut session = Session::new(&self.id);
        session.history = history;
        session.persisted_msg_count = self.persisted_msg_count;
        session.state = self.state.clone().into();
        session.state_stack = self.state_stack.iter().cloned().map(Into::into).collect();
        session
            .extensions
            .restore_cold_snapshot(self.extensions.clone());
        session
    }
}
