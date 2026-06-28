//! Session state machine and registry.
//!
//! Defines the core session types: `AgentState`, `Session`,
//! `SessionRegistry`, and related snapshots.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::effect::{HistoryEdit, HistoryOperation};
use super::fundamental::{BriocheError, SubRoutineHandle};
use super::tool::{ActiveToolCall, ToolCallDescriptor};
use crate::BriocheExtensionType;
use crate::extension::ExtensionStorage;

// AgentState
// ---------------------------------------------------------------------------

/// Mechanical states of the hierarchical automaton.
///
/// `AgentState` contains **only** pure mechanical states. No policy state
/// (quarantine, recovery, timeout) appears here. Governance plugins force
/// transitions via `OverrideTransition` if needed.
///
/// Refs: I-Core-AgentState
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum AgentState {
    /// Waiting for user input.
    #[default]
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
    /// Delegated to a sub-routine.
    SubRoutine(SubRoutineHandle),
    /// Terminal failure state. No further effects are emitted.
    Failure,
}

// ---------------------------------------------------------------------------
// ChatMessage
// ---------------------------------------------------------------------------

/// A single message in the session history.
///
/// `ToolResult` content is serialized JSON of a `ToolOutcome`.
///
/// Refs: I-Core-Pure
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ChatMessage {
    /// System prompt or instruction. Fixed at session start.
    System {
        /// Message text content.
        content: String,
    },
    /// User message. Triggers `Idle -> Predicting`.
    User {
        /// Message text content.
        content: String,
    },
    /// Assistant response, possibly with tool calls.
    Assistant {
        /// Message text content.
        content: String,
        /// Optional reasoning / chain-of-thought text.
        /// Preserved for reasoning models (Qwen, DeepSeek, Claude
        /// extended thinking) so that tool-calling continuity is
        /// maintained across turns. The kernel treats this as
        /// opaque metadata.
        ///
        /// Refs: I-Shell-Runtime-OnlyIO
        #[serde(default)]
        reasoning: Option<String>,
        /// Tool calls emitted by the assistant in this turn.
        /// When non-empty, the message maps to OpenAI's
        /// `assistant` role with both `content` and `tool_calls`.
        /// This keeps the internal model aligned with the wire
        /// format, eliminating adapter merge logic.
        ///
        /// Refs: I-Shell-Runtime-OnlyIO
        #[serde(default)]
        tool_calls: Vec<ToolCallDescriptor>,
    },
    /// Tool call requested by the assistant.
    ToolRequest {
        /// Stable identifier for the tool call or result.
        id: String,
        /// Name of the tool being invoked.
        name: String,
        /// JSON-encoded arguments for the tool call.
        arguments: String,
    },
    /// Serialized result of a tool execution.
    ToolResult {
        /// Stable identifier for the tool call or result.
        id: String,
        /// Message text content.
        content: String,
    },
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// Marker type that makes a containing struct `!Send` and `!Sync`.
///
/// Rust stable does not support negative impls (`impl !Send for T`), so we
/// use `PhantomData<*mut ()>` which is inherently `!Send + !Sync`.
type NotSendSync = std::marker::PhantomData<*mut ()>;

/// Drives global state. The automaton never panics: errors become
/// `BriocheError` or the `Failure` state.
///
/// `Session` is strictly `!Send` and `!Sync`. A single thread owns it.
/// Concurrent mutation is prevented by the type system.
///
/// # Complexity
/// O(1) for construction and field/variant access.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-Pure, I-Core-NoPanic, I-Shell-Session-NoSend
pub struct Session {
    /// Stable identifier for the tool call or result.
    pub id: String,
    /// Chronological message history (user, assistant, tool results).
    pub history: Vec<ChatMessage>,
    /// Disk synchronization index for the Delta protocol (Redb).
    pub persisted_msg_count: usize,
    /// Current mechanical state of the hierarchical automaton.
    pub state: AgentState,
    /// Stack of previous states, restored on `pop_state()`.
    pub state_stack: Vec<AgentState>,
    /// Plugin state container. Typed via `BriocheExtensionType`.
    pub extensions: ExtensionStorage,
    /// Mechanical state: tools currently in execution.
    /// Managed exclusively by the kernel. Not modifiable by plugins.
    pub active_tools: Vec<ActiveToolCall>,
    /// Temporary buffer for accumulating assistant text fragments
    /// during LLM streaming. Materialized into `ChatMessage::Assistant`
    /// on `StreamEvent::Done` or `StreamEvent::ToolCallDone`.
    ///
    /// This field is mechanical: it belongs to the kernel lifecycle
    /// and is never exposed to plugins via `ExtensionStorage`.
    ///
    /// Refs: I-Core-StreamAccumulator, I-Core-Pure
    pub pending_assistant_text: String,
    /// Stable-marker making `Session` `!Send + !Sync`.
    _not_send_sync: NotSendSync,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("history_len", &self.history.len())
            .field("persisted_msg_count", &self.persisted_msg_count)
            .field("state", &self.state)
            .field("state_stack_depth", &self.state_stack.len())
            .field("active_tools", &self.active_tools)
            .field("pending_assistant_len", &self.pending_assistant_text.len())
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Create a new session in `AgentState::Idle`.
    ///
    /// # Complexity
    /// O(1). Allocates empty collections.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-AgentState
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            history: Vec::new(),
            persisted_msg_count: 0,
            state: AgentState::Idle,
            state_stack: Vec::new(),
            extensions: ExtensionStorage::new(),
            active_tools: Vec::new(),
            pending_assistant_text: String::new(),
            _not_send_sync: std::marker::PhantomData,
        }
    }

    /// Push the current state onto the stack and transition to `new_state`.
    ///
    /// # Complexity
    /// O(1). One `Vec` push.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if the transition
    /// is semantically invalid (e.g., pushing `Failure`) or if the stack
    /// would exceed `MAX_STATE_STACK_DEPTH`.
    ///
    /// Refs: I-Core-NoPanic
    pub fn push_state(&mut self, new_state: AgentState) -> Result<(), BriocheError> {
        if matches!(new_state, AgentState::Failure) {
            return Err(BriocheError::InvalidStateTransition(
                "cannot push Failure onto state stack".into(),
            ));
        }
        if self.state_stack.len() >= crate::types::MAX_STATE_STACK_DEPTH {
            return Err(BriocheError::InvalidStateTransition(format!(
                "state stack depth exceeds maximum {}",
                crate::types::MAX_STATE_STACK_DEPTH
            )));
        }
        let old = std::mem::replace(&mut self.state, new_state);
        self.state_stack.push(old);
        Ok(())
    }

    /// Pop the top state from the stack and restore it.
    ///
    /// # Complexity
    /// O(1). One `Vec` pop.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if the stack is empty.
    ///
    /// Refs: I-Core-NoPanic
    pub fn pop_state(&mut self) -> Result<AgentState, BriocheError> {
        match self.state_stack.pop() {
            Some(prev) => {
                let current = std::mem::replace(&mut self.state, prev);
                Ok(current)
            }
            None => Err(BriocheError::InvalidStateTransition(
                "state stack is empty".into(),
            )),
        }
    }

    /// Produce a read-only `SessionSnapshot` for plugin consumption.
    ///
    /// The kernel injects this into `ExtensionStorage` before each hook
    /// so that plugins can read session state without direct field access.
    ///
    /// # Complexity
    /// O(1). No allocation.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-Pure
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            current_state: AgentStateTag::from(&self.state),
            state_stack_depth: self.state_stack.len(),
        }
    }

    /// Persist accumulated assistant text into history, if any.
    ///
    /// Moves `pending_assistant_text` into a `ChatMessage::Assistant` and
    /// clears the buffer. No-op if the buffer is empty.
    ///
    /// # Complexity
    /// O(1). One `Vec` push if text exists.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-ChunkBudget
    pub fn persist_assistant_text(&mut self) {
        if !self.pending_assistant_text.is_empty() {
            self.history.push(ChatMessage::Assistant {
                content: std::mem::take(&mut self.pending_assistant_text),
                reasoning: None,
                tool_calls: Vec::new(),
            });
        }
    }

    /// Apply a sequence of `HistoryEdit`s to the session, validating indices.
    ///
    /// # Complexity
    /// O(e) where e = number of edits. One `Vec` insert/replace per edit,
    /// plus one `drain` for `Truncate`.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if an index is out of bounds.
    ///
    /// Refs: I-Core-NoPanic
    pub fn apply_history_edits(&mut self, edits: &[HistoryEdit]) -> Result<(), BriocheError> {
        for edit in edits {
            match edit {
                HistoryEdit::Insert { index, message } => {
                    if *index > self.history.len() {
                        return Err(BriocheError::HistoryIndexOutOfBounds {
                            operation: HistoryOperation::Insert,
                            index: *index,
                            len: self.history.len(),
                        });
                    }
                    self.history.insert(*index, message.clone());
                }
                HistoryEdit::Replace { index, message } => {
                    if *index >= self.history.len() {
                        return Err(BriocheError::HistoryIndexOutOfBounds {
                            operation: HistoryOperation::Replace,
                            index: *index,
                            len: self.history.len(),
                        });
                    }
                    // Invariant: index validated above.
                    if let Some(slot) = self.history.get_mut(*index) {
                        *slot = message.clone();
                    }
                }
                HistoryEdit::Truncate { keep_last } => {
                    let keep = (*keep_last).min(self.history.len());
                    let drain_count = self.history.len() - keep;
                    self.history.drain(..drain_count);
                }
            }
        }
        Ok(())
    }
}

// Manual Default because ExtensionStorage does not derive Default in the
// same way (it contains HashMap which is Default, but we want explicit).
impl Default for Session {
    fn default() -> Self {
        Self::new("")
    }
}

// ---------------------------------------------------------------------------
// SessionRegistry
// ---------------------------------------------------------------------------

/// Holds live `Session` instances of sub-routines on the synchronous thread.
///
/// The shell and persistence manipulate only flattened `SessionHeadDTO`s.
/// The kernel is the sole holder of live `Session` instances.
///
/// `SessionRegistry` is strictly `!Send` and `!Sync`.
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
///
/// Refs: I-Shell-Session-NoSend, I-Shell-DTO-Only
pub struct SessionRegistry {
    sessions: BTreeMap<SubRoutineHandle, Session>,
    /// Outgoing transition counters per handle.
    /// Incremented at each outgoing transition from `SubRoutine`.
    /// Used by `SubRoutineCleanupGuard` for defensive cleanup.
    exit_counts: BTreeMap<SubRoutineHandle, u64>,
    /// Stable-marker making `SessionRegistry` `!Send + !Sync`.
    _not_send_sync: NotSendSync,
}

impl std::fmt::Debug for SessionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRegistry")
            .field("sessions", &self.sessions.keys().collect::<Vec<_>>())
            .field("exit_counts", &self.exit_counts)
            .finish_non_exhaustive()
    }
}

impl SessionRegistry {
    /// Create an empty registry.
    ///
    /// # Complexity
    /// O(1).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-AgentState
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            exit_counts: BTreeMap::new(),
            _not_send_sync: std::marker::PhantomData,
        }
    }

    /// Insert a sub-routine session.
    ///
    /// # Complexity
    /// O(log n) where n = number of sub-routines.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn insert(&mut self, handle: SubRoutineHandle, session: Session) {
        self.sessions.insert(handle, session);
    }

    /// Get a mutable reference to a sub-routine session.
    ///
    /// # Complexity
    /// O(log n).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn get_mut(&mut self, handle: &SubRoutineHandle) -> Option<&mut Session> {
        self.sessions.get_mut(handle)
    }

    /// Remove a sub-routine session, returning it if present.
    ///
    /// # Complexity
    /// O(log n).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn remove(&mut self, handle: &SubRoutineHandle) -> Option<Session> {
        self.sessions.remove(handle)
    }

    /// Returns `true` if the registry contains the given handle.
    ///
    /// # Complexity
    /// O(log n).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn contains(&self, handle: &SubRoutineHandle) -> bool {
        self.sessions.contains_key(handle)
    }

    /// Increment the exit counter for a sub-routine handle.
    ///
    /// Called by the kernel on every outgoing transition from `SubRoutine`.
    ///
    /// # Complexity
    /// O(log n).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn increment_exit_count(&mut self, handle: &SubRoutineHandle) {
        *self.exit_counts.entry(handle.clone()).or_insert(0) += 1;
    }

    /// Get the current exit count for a handle.
    ///
    /// # Complexity
    /// O(log n).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn get_exit_count(&self, handle: &SubRoutineHandle) -> u64 {
        match self.exit_counts.get(handle) {
            Some(&v) => v,
            None => 0,
        }
    }

    /// Iterate over all registered handles.
    ///
    /// # Complexity
    /// O(1) for the iterator creation.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn handles(&self) -> impl Iterator<Item = &SubRoutineHandle> {
        self.sessions.keys()
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SessionSnapshot
// ---------------------------------------------------------------------------

/// Tag enum for `SessionSnapshot`, exposing only the mechanical state label.
///
/// This is intentionally a separate type from `AgentState` so that plugins
/// cannot observe or match on internal state data (e.g., `generation_id`).
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Core-AgentState
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStateTag {
    #[default]
    /// No active prediction or tool execution.
    Idle,
    /// LLM response is being streamed.
    Predicting,
    /// Shell is executing tools in parallel.
    ExecutingTools,
    /// Control delegated to a child session.
    SubRoutine,
    /// Terminal failure. Session is dead.
    Failure,
}

impl AgentState {
    /// Extract the generation ID if currently predicting or executing tools.
    ///
    /// Returns `None` for states that carry no generation context.
    ///
    /// # Complexity
    /// O(1). One pattern match.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-NoPanic
    pub fn generation_id(&self) -> Option<u64> {
        match self {
            AgentState::Predicting { generation_id }
            | AgentState::ExecutingTools { generation_id } => Some(*generation_id),
            _ => None,
        }
    }
}

impl From<&AgentState> for AgentStateTag {
    fn from(state: &AgentState) -> Self {
        match state {
            AgentState::Idle => Self::Idle,
            AgentState::Predicting { .. } => Self::Predicting,
            AgentState::ExecutingTools { .. } => Self::ExecutingTools,
            AgentState::SubRoutine(_) => Self::SubRoutine,
            AgentState::Failure => Self::Failure,
        }
    }
}

/// Read-only view of session state exposed to plugins via `ExtensionStorage`.
///
/// The kernel injects this before each hook so plugins can observe state
/// without direct `session.state` access.
///
/// ## Snapshot strategy
/// COW: full clone (~64 bytes). Lightweight — three scalar fields.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Core-Pure
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
pub struct SessionSnapshot {
    /// Mechanical state tag (no internal data exposed to plugins).
    pub current_state: AgentStateTag,
    /// Depth of the state stack (used by depth guards).
    pub state_stack_depth: usize,
}
