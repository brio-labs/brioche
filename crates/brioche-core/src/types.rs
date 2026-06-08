//! Book I — The Core Book: Fundamental types for the Brioche kernel.
//!
//! This module contains `Session`, `AgentState`, `EngineInput`, `Effect`, and
//! related mechanical types. Definitions are populated incrementally across
//! Sprints 2–5.
//!
//! Invariants upheld:
//! - I-Core-Pure: All types are deterministic and serializable.
//! - I-Core-NoPanic: Invalid state transitions produce `BriocheError`, not panics.
//! - I-Core-ActiveToolCall: `ActiveToolCall` is kernel-internal; plugins use `ToolCallDescriptor`.
//! - I-Core-RetVecEffect: `Effect` is the sole output channel of `transition()`.
//!
//! Refs: SPECS.md §2, §5

use crate::BriocheExtensionType;
use crate::extension::ExtensionStorage;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default tool timeout applied when a descriptor omits `timeout_ms`.
///
/// Refs: I-Core-ActiveToolCall
pub const DEFAULT_TOOL_TIMEOUT_MS: u64 = 30_000;

/// Initial generation ID for predictions.
///
/// Refs: I-Core-AgentState
pub const INITIAL_GENERATION_ID: u64 = 1;

/// Maximum number of entries retained in transition trace ring buffers.
///
/// Refs: I-Gov-OverrideTrace
pub const TRACE_LOG_CAPACITY: usize = 128;

/// Maximum size of an inline streaming chunk in bytes.
///
/// SSE payloads exceeding this size are segmented into independent
/// 4 KB fragments before injection into the kernel, guaranteeing the
/// absence of heap allocation in the synchronous hot path for plugins
/// in `Pass` or `Hold` mode.
///
/// Refs: I-Core-ChunkBudget
pub const MAX_INLINE_CHUNK: usize = 4096;

// ---------------------------------------------------------------------------
// Sub-routine handle
// ---------------------------------------------------------------------------

/// Opaque handle identifying a sub-routine session in the `SessionRegistry`.
///
/// `SubRoutineHandle` is `Ord` so it can be used as a `BTreeMap` key,
/// guaranteeing deterministic ordering.
///
/// Refs: I-Core-PluginOrder
///
/// Refs: I-Core-AgentState
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SubRoutineHandle(String);

impl SubRoutineHandle {
    /// Create a new handle from any string-like value.
    ///
    /// Complexity: O(length of id). Allocates one `String`.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if the id is empty,
    /// consists only of whitespace, or contains control characters.
    ///
    /// Refs: I-Core-NoPanic
    pub fn new(id: impl Into<String>) -> Result<Self, BriocheError> {
        let s = id.into();
        if s.is_empty() {
            return Err(BriocheError::InvalidStateTransition(
                "SubRoutineHandle must not be empty".into(),
            ));
        }
        if s.trim().is_empty() {
            return Err(BriocheError::InvalidStateTransition(
                "SubRoutineHandle must not be whitespace-only".into(),
            ));
        }
        if s.chars().any(|c| c.is_control()) {
            return Err(BriocheError::InvalidStateTransition(
                "SubRoutineHandle must not contain control characters".into(),
            ));
        }
        Ok(Self(s))
    }

    /// Borrow the underlying string.
    ///
    /// Complexity: O(1).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// AgentState
// ---------------------------------------------------------------------------

/// Mechanical states of the hierarchical automaton.
///
/// `AgentState` contains **only** pure mechanical states. No policy state
/// (quarantine, recovery, timeout) appears here. Governance plugins force
/// transitions via `OverrideTransition` if needed.
///
/// Refs: I-Core-AgentState
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentState {
    /// Waiting for user input.
    #[default]
    Idle,
    /// LLM prediction in progress.
    Predicting { generation_id: u64 },
    /// Tools are being executed by the shell.
    ExecutingTools { generation_id: u64 },
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
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
    },
    ToolRequest {
        id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        id: String,
        tool_name: String,
        outcome: ToolOutcome,
    },
}

// ---------------------------------------------------------------------------
// Tool descriptors
// ---------------------------------------------------------------------------

/// Tool call descriptor — the plugin-facing interface for tool calls.
///
/// Plugins inspect and mutate `ToolCallDescriptor` via the `on_tool_calls`
/// hook. The kernel converts these into `ActiveToolCall` via `seal()`.
///
/// Refs: I-Core-ActiveToolCall
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallDescriptor {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: String,
    /// Timeout proposed by AI or mutated by policy plugins.
    /// The kernel materializes the final value in `ActiveToolCall.timeout_ms`.
    pub timeout_ms: Option<u64>,
}

/// Kernel-internal representation of a tool call after `seal()`.
///
/// This type is **not** constructible by plugins. It is produced exclusively
/// by the kernel's `seal()` function after the `on_tool_calls` hook.
///
/// Refs: I-Core-ActiveToolCall
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveToolCall {
    pub tool_id: String,
    pub tool_name: String,
    pub arguments: String,
    /// Materialized by the kernel after `on_tool_calls` hook execution.
    pub timeout_ms: u64,
}

/// Canonical conversion from interface type to mechanical type.
///
/// Called immediately after `handle_tool_calls`. Any new field must be mapped
/// explicitly here; the Rust compiler forces exhaustive matching.
///
/// Complexity: O(n) where n = number of descriptors. Allocates one `Vec`.
///
/// Refs: I-Core-ActiveToolCall
pub fn seal(descriptors: Vec<ToolCallDescriptor>, default_timeout_ms: u64) -> Vec<ActiveToolCall> {
    descriptors
        .into_iter()
        .map(|d| ActiveToolCall {
            tool_id: d.tool_id,
            tool_name: d.tool_name,
            arguments: d.arguments,
            timeout_ms: d.timeout_ms.unwrap_or(default_timeout_ms),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tool outcome
// ---------------------------------------------------------------------------

/// Business result of a tool execution.
///
/// These are **data**, not failures. The LLM receives them in context
/// and can react accordingly.
///
/// Refs: SPECS.md §1.5
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolOutcome {
    Success(String),
    BusinessError(String),
    SystemError(String),
    TimeoutWithPartialData { partial_output: Option<String> },
}

impl ToolOutcome {
    /// Returns the string content of the outcome, if any.
    ///
    /// For `TimeoutWithPartialData`, returns the partial output or an empty string.
    pub fn content(&self) -> &str {
        match self {
            ToolOutcome::Success(s)
            | ToolOutcome::BusinessError(s)
            | ToolOutcome::SystemError(s) => s,
            ToolOutcome::TimeoutWithPartialData { partial_output } => {
                partial_output.as_deref().unwrap_or("")
            }
        }
    }
}

/// Structured result returned from the shell to the kernel after tool execution.
///
/// Refs: I-Core-Pure
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultDTO {
    pub tool_id: String,
    pub tool_name: String,
    pub outcome: ToolOutcome,
}

/// Structured truncation metadata for oversized tool results.
///
/// Replaces hand-rolled JSON `format!()` with a typed domain object
/// that serializes deterministically via `serde_json`.
///
/// Refs: I-Comp-Pure-Logic, I-Comp-Typed-Effects
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TruncatedToolResult {
    pub truncated: bool,
    pub original_len: usize,
    pub preview: String,
}

impl TruncatedToolResult {
    /// Creates a truncation record from the full content and a byte limit.
    ///
    /// Complexity: O(1). One `String` allocation for the preview.
    pub fn from_content(content: &str, max_bytes: usize) -> Self {
        let byte_limit = max_bytes.min(content.len());
        // Find the nearest char boundary at or before byte_limit to avoid panics.
        let safe_limit = if byte_limit == content.len() {
            byte_limit
        } else {
            let mut boundary = byte_limit;
            while boundary > 0 && !content.is_char_boundary(boundary) {
                boundary -= 1;
            }
            boundary
        };
        let preview = content[..safe_limit].to_string();
        Self {
            truncated: true,
            original_len: content.len(),
            preview,
        }
    }

    /// Serializes to a JSON string for injection into `ToolOutcome::Success`.
    ///
    /// Complexity: O(n) where n = JSON length. One `String` allocation.
    ///
    /// # Errors
    /// Returns `BriocheError::Serialization` if JSON serialization fails.
    pub fn to_json(&self) -> Result<String, BriocheError> {
        serde_json::to_string(self)
            .map_err(|e| BriocheError::Serialization(format!("TruncatedToolResult: {e}")))
    }
}

/// Event log for COW rollback telemetry.
///
/// Written by `CycleRollbackPolicy` implementations during `commit_hook`
/// and `rollback_hook`, then consumed by `RollbackTelemetryEmitter`.
///
/// Refs: I-Gov-Rollback-BestEffort, I-Comp-Pure-Logic
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct RollbackEventLog {
    /// Events recorded since the last consumption.
    #[brioche(deterministic_order)]
    pub events: Vec<RollbackEvent>,
}

/// Single COW rollback event.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RollbackEvent {
    /// Hook name during which the event occurred.
    pub hook_name: String,
    /// `true` = rollback restored snapshots; `false` = commit discarded them.
    pub was_rollback: bool,
    /// Cumulative weight of the frame at decision time (bytes).
    pub frame_weight: usize,
    /// Whether the budget was exceeded (abandoned rollback).
    pub budget_exceeded: bool,
}

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
/// All mutable access to mechanical state must go through the provided
/// accessor methods so that invariants are upheld.
///
/// Refs: I-Core-Pure, I-Core-NoPanic, I-Shell-Session-NoSend
pub struct Session {
    id: String,
    history: Vec<ChatMessage>,
    /// Disk synchronization index for the Delta protocol (Redb).
    persisted_msg_count: usize,
    state: AgentState,
    state_stack: Vec<AgentState>,
    extensions: ExtensionStorage,
    active_tools: Vec<ActiveToolCall>,
    /// Temporary buffer to accumulate assistant text fragments during LLM
    /// streaming. Materialized into `ChatMessage::Assistant` on
    /// `StreamEvent::Done` or `StreamEvent::ToolCallDone`.
    ///
    /// Stored as raw bytes so that split UTF-8 characters at chunk
    /// boundaries do not corrupt the stream. Converted to `String` only
    /// at materialization points when the complete payload is known.
    ///
    /// Refs: I-Core-StreamAccumulator, I-Core-Pure, I-Core-ChunkBudget
    pending_assistant_bytes: Vec<u8>,
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
            .field(
                "pending_assistant_bytes",
                &self.pending_assistant_bytes.len(),
            )
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Create a new session in `AgentState::Idle`.
    ///
    /// Complexity: O(1). Allocates empty collections.
    pub fn new(id: impl Into<String>) -> Self {
        let mut s = Self {
            id: id.into(),
            history: Vec::new(),
            persisted_msg_count: 0,
            state: AgentState::Idle,
            state_stack: Vec::new(),
            extensions: ExtensionStorage::new(),
            active_tools: Vec::new(),
            pending_assistant_bytes: Vec::new(),
            _not_send_sync: std::marker::PhantomData,
        };
        // Pre-warm the SessionSnapshot slot to avoid per-hook allocations.
        s.extensions.insert_hot(SessionSnapshot::default());
        s
    }

    /// Borrow the session identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Immutable view of the message history.
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Number of messages in history.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Append a message to history.
    pub fn push_history(&mut self, msg: ChatMessage) {
        self.history.push(msg);
    }

    /// Insert a message at `index`, validating bounds.
    pub fn insert_history(&mut self, index: usize, msg: ChatMessage) -> Result<(), BriocheError> {
        if index > self.history.len() {
            return Err(BriocheError::InvalidStateTransition(format!(
                "history insert index {} out of bounds (len={})",
                index,
                self.history.len()
            )));
        }
        self.history.insert(index, msg);
        Ok(())
    }

    /// Replace a message at `index`, validating bounds.
    pub fn replace_history(&mut self, index: usize, msg: ChatMessage) -> Result<(), BriocheError> {
        if index >= self.history.len() {
            return Err(BriocheError::InvalidStateTransition(format!(
                "history replace index {} out of bounds (len={})",
                index,
                self.history.len()
            )));
        }
        self.history[index] = msg;
        Ok(())
    }

    /// Replace the entire message history.
    ///
    /// Used by the persistence layer during session hydration.
    /// The caller is responsible for ensuring the replacement history
    /// upholds session invariants.
    pub fn set_history(&mut self, history: Vec<ChatMessage>) {
        self.history = history;
    }

    /// Insert a message at `index` without bounds checking.
    ///
    /// # Safety
    /// `index` must be <= `self.history.len()`.
    pub unsafe fn insert_history_unchecked(&mut self, index: usize, msg: ChatMessage) {
        self.history.insert(index, msg);
    }

    /// Replace a message at `index` without bounds checking.
    ///
    /// # Safety
    /// `index` must be < `self.history.len()`.
    pub unsafe fn replace_history_unchecked(&mut self, index: usize, msg: ChatMessage) {
        self.history[index] = msg;
    }

    /// Remove and return the last message from history, if any.
    ///
    /// Complexity: O(1).
    pub fn pop_history(&mut self) -> Option<ChatMessage> {
        self.history.pop()
    }

    /// Truncate history, keeping only the last `keep_last` messages.
    pub fn truncate_history(&mut self, keep_last: usize) {
        let keep = keep_last.min(self.history.len());
        let drain_count = self.history.len() - keep;
        self.history.drain(..drain_count);
    }

    /// Disk synchronization index for the Delta protocol.
    pub fn persisted_msg_count(&self) -> usize {
        self.persisted_msg_count
    }

    /// Set the disk synchronization index.
    pub fn set_persisted_msg_count(&mut self, count: usize) {
        self.persisted_msg_count = count;
    }

    /// Current mechanical state.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Restore session state and stack from a DTO or test fixture.
    ///
    /// This is the **only** public API for direct state replacement.
    /// Governance plugins must NEVER call this; they should return
    /// `OverrideTransition` or `ConsistencyReport` effects instead.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Persist-Idempotence, I-Core-NoPanic
    pub fn restore_state(&mut self, state: AgentState, stack: Vec<AgentState>) {
        self.state = state;
        self.state_stack = stack;
    }

    /// Mutable access to the current state.
    ///
    /// # Safety
    /// Direct mutation bypasses `push_state`/`pop_state` invariant checks.
    ///
    /// This is exposed so that mechanism code (e.g. the engine applying a
    /// `ConsistencyReport`) can perform emergency state repair. Policy plugins
    /// must NOT call this; they should emit `OverrideTransition` instead.
    ///
    /// # Safety
    /// Direct mutation can corrupt the automaton. The caller is responsible
    /// for leaving the session in a consistent state.
    pub(crate) fn set_state(&mut self, state: AgentState) {
        self.state = state;
    }

    /// Push the current state onto the stack and transition to `new_state`.
    ///
    /// Used when entering `Predicting` or `ExecutingTools` with context
    /// that must later be restored.
    ///
    /// Complexity: O(1). One `Vec` push.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if the transition
    /// is semantically invalid (e.g., pushing `Failure`).
    ///
    /// Refs: I-Core-NoPanic
    pub fn push_state(&mut self, new_state: AgentState) -> Result<(), BriocheError> {
        if matches!(new_state, AgentState::Failure) {
            return Err(BriocheError::InvalidStateTransition(
                "cannot push Failure onto state stack".into(),
            ));
        }
        let old = std::mem::replace(&mut self.state, new_state);
        self.state_stack.push(old);
        Ok(())
    }

    /// Pop the top state from the stack and restore it.
    ///
    /// Complexity: O(1). One `Vec` pop.
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

    /// Depth of the state stack.
    pub fn state_stack_depth(&self) -> usize {
        self.state_stack.len()
    }

    /// Immutable view of the state stack.
    pub fn state_stack(&self) -> &Vec<AgentState> {
        &self.state_stack
    }

    /// Clear the state stack without restoring state.
    ///
    /// Used by the engine when applying a `ConsistencyReport` fix.
    pub(crate) fn clear_state_stack(&mut self) {
        self.state_stack.clear();
    }

    /// Immutable view of the extension storage.
    pub fn extensions(&self) -> &ExtensionStorage {
        &self.extensions
    }

    /// Mutable view of the extension storage.
    ///
    /// The caller is responsible for upholding COW and determinism
    /// invariants. Prefer kernel-mediated access whenever possible.
    pub fn extensions_mut(&mut self) -> &mut ExtensionStorage {
        &mut self.extensions
    }

    /// Immutable view of active tool calls.
    pub fn active_tools(&self) -> &[ActiveToolCall] {
        &self.active_tools
    }

    /// Replace the active tool calls list.
    pub fn set_active_tools(&mut self, tools: Vec<ActiveToolCall>) {
        self.active_tools = tools;
    }

    /// Clear the active tool calls list.
    pub fn clear_active_tools(&mut self) {
        self.active_tools.clear();
    }

    /// Immutable view of accumulated assistant bytes.
    pub fn pending_assistant_bytes(&self) -> &[u8] {
        &self.pending_assistant_bytes
    }

    /// Append raw bytes to the assistant accumulator.
    pub fn append_assistant_bytes(&mut self, bytes: &[u8]) {
        self.pending_assistant_bytes.extend_from_slice(bytes);
    }

    /// Clear the assistant accumulator.
    pub fn clear_assistant_bytes(&mut self) {
        self.pending_assistant_bytes.clear();
    }

    /// Mutable view of accumulated assistant bytes.
    ///
    /// Used by the kernel to take ownership of the buffer at
    /// materialization boundaries.
    pub fn pending_assistant_bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.pending_assistant_bytes
    }

    /// Take and return all accumulated assistant bytes, leaving the buffer empty.
    pub fn take_assistant_bytes(&mut self) -> Vec<u8> {
        std::mem::take(self.pending_assistant_bytes_mut())
    }

    /// Convert accumulated bytes to a `String`.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if the bytes are not
    /// valid UTF-8.
    ///
    /// Refs: I-Core-Pure
    pub fn assistant_text(&self) -> Result<String, BriocheError> {
        String::from_utf8(self.pending_assistant_bytes.clone()).map_err(|e| {
            BriocheError::InvalidStateTransition(format!(
                "assistant bytes contain invalid UTF-8: {e}"
            ))
        })
    }

    /// Append text to the assistant accumulator.
    pub fn append_assistant_text(&mut self, text: &str) {
        self.pending_assistant_bytes
            .extend_from_slice(text.as_bytes());
    }

    /// Take and return accumulated assistant text, leaving the buffer empty.
    ///
    /// # Errors
    /// Returns `BriocheError::InvalidStateTransition` if the bytes are not
    /// valid UTF-8.
    ///
    /// Refs: I-Core-Pure
    pub fn take_assistant_text(&mut self) -> Result<String, BriocheError> {
        let bytes = std::mem::take(&mut self.pending_assistant_bytes);
        String::from_utf8(bytes).map_err(|e| {
            BriocheError::InvalidStateTransition(format!(
                "assistant bytes contain invalid UTF-8: {e}"
            ))
        })
    }

    /// Returns `true` if no assistant bytes have been accumulated.
    pub fn is_pending_assistant_empty(&self) -> bool {
        self.pending_assistant_bytes.is_empty()
    }

    /// Produce a read-only `SessionSnapshot` for plugin consumption.
    ///
    /// The kernel injects this into `ExtensionStorage` before each hook
    /// so that plugins can read session state without direct field access.
    ///
    /// Complexity: O(1). No allocation.
    ///
    /// Refs: I-Core-Pure
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            current_state: AgentStateTag::from(&self.state),
            state_stack_depth: self.state_stack.len(),
        }
    }
}

// Manual Default for explicit construction.
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
    /// Complexity: O(1).
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            exit_counts: BTreeMap::new(),
            _not_send_sync: std::marker::PhantomData,
        }
    }

    /// Insert a sub-routine session.
    ///
    /// Returns the previous session if one existed with the same handle.
    ///
    /// Complexity: O(log n) where n = number of sub-routines.
    pub fn insert(&mut self, handle: SubRoutineHandle, session: Session) -> Option<Session> {
        self.sessions.insert(handle, session)
    }

    /// Get a mutable reference to a sub-routine session.
    ///
    /// Complexity: O(log n).
    pub fn get_mut(&mut self, handle: &SubRoutineHandle) -> Option<&mut Session> {
        self.sessions.get_mut(handle)
    }

    /// Remove a sub-routine session, returning it if present.
    ///
    /// Complexity: O(log n).
    pub fn remove(&mut self, handle: &SubRoutineHandle) -> Option<Session> {
        self.sessions.remove(handle)
    }

    /// Returns `true` if the registry contains the given handle.
    ///
    /// Complexity: O(log n).
    pub fn contains(&self, handle: &SubRoutineHandle) -> bool {
        self.sessions.contains_key(handle)
    }

    /// Increment the exit counter for a sub-routine handle.
    ///
    /// Called by the kernel on every outgoing transition from `SubRoutine`.
    ///
    /// Complexity: O(log n).
    pub fn increment_exit_count(&mut self, handle: &SubRoutineHandle) {
        *self.exit_counts.entry(handle.clone()).or_insert(0) += 1;
    }

    /// Get the current exit count for a handle.
    ///
    /// Complexity: O(log n).
    pub fn get_exit_count(&self, handle: &SubRoutineHandle) -> u64 {
        self.exit_counts.get(handle).copied().unwrap_or(0)
    }

    /// Iterate over all registered handles.
    ///
    /// Complexity: O(1) for the iterator creation.
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
/// Refs: I-Core-AgentState
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStateTag {
    #[default]
    Idle,
    Predicting,
    ExecutingTools,
    SubRoutine,
    Failure,
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
/// `#[brioche(no_snapshot)]` — fully reconstructed each cycle; COW
/// rollback of this slot is meaningless and would waste hot-path budget.
///
/// Refs: I-Core-Pure, I-Core-ChunkBudget
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct SessionSnapshot {
    pub current_state: AgentStateTag,
    pub state_stack_depth: usize,
}

// ---------------------------------------------------------------------------
// EngineInput
// ---------------------------------------------------------------------------

/// High-level input to the synchronous kernel.
///
/// System signals, async results, and governance notifications transit
/// through **separate channels** (see SPECS.md §1.4) and are **not**
/// variants of `EngineInput`.
///
/// Refs: I-Core-EngineInput
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineInput {
    /// User message. Triggers `Idle -> Predicting` transition.
    UserMessage(String),
    /// LLM stream fragments.
    LlmStream(StreamEvent),
    /// Tool execution results (parallelized by the shell).
    ToolCallsResult {
        generation_id: u64,
        results: Vec<ToolResultDTO>,
    },
    /// Request to create a sub-routine in the `SessionRegistry`.
    ///
    /// Sprint 5: blob hydration is handled by the shell before this input
    /// is emitted; the engine receives only the handle.
    RestoreSubRoutine { handle: SubRoutineHandle },
}

// ---------------------------------------------------------------------------
// PolicyDecision
// ---------------------------------------------------------------------------

/// Decision returned by a plugin hook, interpreted by the kernel.
///
/// Refs: I-Gov-Decision-Required, I-Gov-OverrideTrace
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Allow the current operation to proceed.
    Allow,
    /// Block the current operation with a reason.
    Block { reason: String },
    /// Mutate the session history before the next phase.
    MutateHistory(Vec<HistoryEdit>),
    /// Request emission of a mechanical effect.
    /// Validated by `HookEffectConstraint` if injected.
    RequestEffect(Effect),
    /// Force a state transition and emit associated effects.
    OverrideTransition(Vec<Effect>),
}

/// Individual history edit operation.
///
/// Applied sequentially in plugin evaluation order. The kernel validates
/// indices after each edit to prevent out-of-bounds mutations.
///
/// Refs: I-Gov-Decision-Isolation
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryEdit {
    Insert { index: usize, message: ChatMessage },
    Replace { index: usize, message: ChatMessage },
    Truncate { keep_last: usize },
}

// ---------------------------------------------------------------------------
// UiWidget
// ---------------------------------------------------------------------------

/// Structured UI widget emitted via `Effect::ForwardToUi`.
///
/// Replaces the previous `String` + `serde_json::Value` anti-pattern with
/// exhaustively matchable domain types. Third-party widgets that do not
/// match a known shape fall back to `UiWidget::Custom`.
///
/// The projection layer can still match on canonical widget type strings
/// via `UiWidget::widget_type()` during migration; new code should match
/// on enum variants directly.
///
/// Refs: I-Comp-Typed-Effects
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiWidget {
    /// Text fragment from LLM streaming.
    TextChunk { trace_id: String, text: String },
    /// Generic error notification displayed in the content area.
    Error { code: String, message: String },
    /// Critical system error (e.g., governance cascade failure).
    CriticalError {
        component: String,
        detail: Option<String>,
    },
    /// System degradation banner (e.g., plugin quarantined).
    SystemDegraded { plugin: String },
    /// Network unavailability notification.
    NetworkError { reason: String },
    /// Generic status indicator (e.g., "cancelled").
    Status(String),
    /// Sub-routine timeout notification.
    SubRoutineTimeout {
        handle: SubRoutineHandle,
        limit_ms: u64,
    },
    /// Sub-routine successfully restored.
    SubRoutineLoaded { handle: SubRoutineHandle },
    /// Pending task status update.
    PendingTask { task_id: String, status: String },
    /// Test widget for integration tests.
    Test { msg: String },
    /// Catch-all for unknown third-party widgets.
    Custom {
        widget_type: String,
        payload: Vec<u8>,
    },
}

impl UiWidget {
    /// Returns the canonical widget type string.
    ///
    /// Used by the projection layer for registry lookup and priority
    /// classification while the ecosystem migrates to structured variants.
    ///
    /// Complexity: O(1).
    pub fn widget_type(&self) -> &str {
        match self {
            UiWidget::TextChunk { .. } => "text_chunk",
            UiWidget::Error { .. } => "error",
            UiWidget::CriticalError { .. } => "critical_error",
            UiWidget::SystemDegraded { .. } => "system_degraded",
            UiWidget::NetworkError { .. } => "network_error",
            UiWidget::Status(_) => "status",
            UiWidget::SubRoutineTimeout { .. } => "subroutine_timeout",
            UiWidget::SubRoutineLoaded { .. } => "subroutine_loaded",
            UiWidget::PendingTask { .. } => "pending_task",
            UiWidget::Test { .. } => "test",
            UiWidget::Custom { widget_type, .. } => widget_type,
        }
    }
}

// ---------------------------------------------------------------------------
// Effect
// ---------------------------------------------------------------------------

/// Declarative effect emitted by the kernel. The shell is responsible for
/// execution.
///
/// `Effect` contains **only** pure mechanical effects. No telemetry,
/// UI fallback, or specific notification variants appear here.
///
/// Refs: I-Core-EffectPure, I-Core-RetVecEffect
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    CallLlmNetwork,
    ExecuteTools(Vec<ActiveToolCall>),
    ForwardToUi(UiWidget),
    Error {
        code: ErrorCode,
        message: String,
    },
    SaveSession,
    SavePluginBlob {
        plugin_id: String,
        data: Vec<u8>,
    },
    TriggerSummarization,
    ExecuteCpuTask {
        task_id: String,
        payload: Vec<u8>,
    },
    TriggerGc,
    SystemIdle,
    PluginFault {
        plugin_name: String,
        error: PluginError,
    },
    RebuildRoutes,
    SubRoutineRestored {
        handle: SubRoutineHandle,
    },
}

/// Mechanical error codes carried by `Effect::Error`.
///
/// These are **not** plugin errors; they represent system-level conditions
/// that the shell must handle.
///
/// Refs: I-Core-NoPanic
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorCode {
    NetworkUnavailable,
    OperationCancelled,
    StateInconsistency,
    EpochMismatch,
    PluginFaulted,
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

/// Execution path for nested / tree-structured stream events.
///
/// `Arc<Vec<String>>` makes clone O(1) — the shell builds the path once
/// and the kernel clones the `Arc` for each chunk, eliminating the
/// per-chunk allocation spike noted in earlier sprints.
///
/// Refs: I-Core-ChunkBudget
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPath {
    pub nodes: std::sync::Arc<Vec<String>>,
}

/// Stream event delivered by the LLM provider.
///
/// `Bytes` is used for text fragments to avoid heap allocations in the
/// synchronous hot path. SSE payloads are pre-segmented to `MAX_INLINE_CHUNK`
/// (4096 bytes) by the shell.
///
/// Refs: I-Core-ChunkBudget, I-Core-StreamNoBranch
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamEvent {
    TextChunk {
        path: ExecutionPath,
        chunk: Bytes,
    },
    ToolCallStart {
        path: ExecutionPath,
        id: String,
        name: String,
    },
    ToolArgumentChunk {
        path: ExecutionPath,
        id: String,
        chunk: Bytes,
    },
    ToolCallDone {
        path: ExecutionPath,
    },
    /// End-of-stream marker. Sent by the shell when the LLM response
    /// completes without further chunks or tool calls.
    Done,
}

/// Action requested by a plugin in response to a stream event.
///
/// Refs: I-Core-StreamNoBranch
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamAction {
    /// Let the chunk pass through.
    Pass,
    /// Hold the chunk (buffering).
    Hold,
    /// Offload a CPU-intensive task to the shell.
    OffloadTask { task_id: String, payload: Vec<u8> },
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Policy error emitted by plugins.
///
/// - `Soft`: minor error. Logged; evaluation continues.
/// - `Fatal`: structural error. The kernel emits `Effect::PluginFault`.
///
/// Refs: SPECS.md §1.5
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum PluginError {
    #[error("soft error in plugin {plugin_name}: {message}")]
    Soft {
        plugin_name: String,
        message: String,
    },
    #[error("fatal error in plugin {plugin_name}: {message}")]
    Fatal {
        plugin_name: String,
        message: String,
    },
}

/// System error — internal monolith failure.
///
/// These are never panics; they are returned as `Result::Err` and
/// typically converted into `Effect::Error` or `AgentState::Failure`.
///
/// Refs: I-Core-NoPanic, SPECS.md §1.5
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[non_exhaustive]
pub enum BriocheError {
    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),
    #[error("storage access failed: {0}")]
    StorageAccess(String),
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("plugin not found: {0}")]
    PluginNotFound(String),
    #[error("other error: {0}")]
    Other(String),
}

/// Convenience alias for plugin hook results.
///
/// Refs: I-Gov-NoCoreMutation
pub type PluginResult<T> = Result<T, PluginError>;

// ---------------------------------------------------------------------------
// EpochAction
// ---------------------------------------------------------------------------

/// Result of epoch interception by the `EpochInterceptor` governance trait.
///
/// Refs: I-Gov-Epoch-Reject
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpochAction {
    /// Input is valid for the current epoch; proceed with standard dispatch.
    Proceed,
    /// Input belongs to a past epoch; reject silently.
    Block { reason: String },
}

// ---------------------------------------------------------------------------
// EffectBit
// ---------------------------------------------------------------------------

/// Bitmask constants for each `Effect` variant, used by `HookEffectConstraint`
/// for O(1) permission validation.
///
/// Refs: I-Core-HookEffect-O1
pub struct EffectBit;

impl EffectBit {
    pub const CALL_LLM_NETWORK: u64 = 1 << 0;
    pub const EXECUTE_TOOLS: u64 = 1 << 1;
    pub const FORWARD_TO_UI: u64 = 1 << 2;
    pub const ERROR: u64 = 1 << 3;
    pub const SAVE_SESSION: u64 = 1 << 4;
    pub const SAVE_PLUGIN_BLOB: u64 = 1 << 5;
    pub const TRIGGER_SUMMARIZATION: u64 = 1 << 6;
    pub const EXECUTE_CPU_TASK: u64 = 1 << 7;
    pub const TRIGGER_GC: u64 = 1 << 8;
    pub const SYSTEM_IDLE: u64 = 1 << 9;
    pub const PLUGIN_FAULT: u64 = 1 << 10;
    pub const REBUILD_ROUTES: u64 = 1 << 11;
    pub const SUB_ROUTINE_RESTORED: u64 = 1 << 12;
    // Bits 13-63 reserved for future extensions.
}

/// Map an `Effect` to its bitmask constant.
///
/// Complexity: O(1). Match on enum variant.
///
/// Refs: I-Core-HookEffect-O1
pub fn effect_to_bitmask(effect: &Effect) -> u64 {
    match effect {
        Effect::CallLlmNetwork => EffectBit::CALL_LLM_NETWORK,
        Effect::ExecuteTools(_) => EffectBit::EXECUTE_TOOLS,
        Effect::ForwardToUi(_) => EffectBit::FORWARD_TO_UI,
        Effect::Error { .. } => EffectBit::ERROR,
        Effect::SaveSession => EffectBit::SAVE_SESSION,
        Effect::SavePluginBlob { .. } => EffectBit::SAVE_PLUGIN_BLOB,
        Effect::TriggerSummarization => EffectBit::TRIGGER_SUMMARIZATION,
        Effect::ExecuteCpuTask { .. } => EffectBit::EXECUTE_CPU_TASK,
        Effect::TriggerGc => EffectBit::TRIGGER_GC,
        Effect::SystemIdle => EffectBit::SYSTEM_IDLE,
        Effect::PluginFault { .. } => EffectBit::PLUGIN_FAULT,
        Effect::RebuildRoutes => EffectBit::REBUILD_ROUTES,
        Effect::SubRoutineRestored { .. } => EffectBit::SUB_ROUTINE_RESTORED,
    }
}

// ---------------------------------------------------------------------------
// Trace types (for OverrideTransition traceability)
// ---------------------------------------------------------------------------

/// Single entry in the `TransitionTraceLog` ring buffer.
///
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionTrace {
    pub source_plugin: String,
    pub decision: PolicyDecision,
    pub epoch: u64,
}

/// Ring buffer for traceability of applied `OverrideTransition`s (max 128 entries, FIFO).
///
/// ## Snapshot strategy
/// COW: full clone. Weight ~O(n) where n = entries (max 128).
///
/// Refs: I-Gov-OverrideTrace, I-Core-NoPanic
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct TransitionTraceLog {
    pub entries: VecDeque<TransitionTrace>,
}

/// Single entry in the `SupersededTransitionTraceLog` ring buffer.
///
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersededTransitionTrace {
    pub source_plugin: String,
    pub attempted_decision: PolicyDecision,
    pub preempted_by: String,
    pub epoch: u64,
}

/// Ring buffer of preempted `OverrideTransition`s (max 128 entries, FIFO).
///
/// ## Snapshot strategy
/// COW: full clone. Weight ~O(n) where n = entries (max 128).
///
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct SupersededTransitionTraceLog {
    pub entries: VecDeque<SupersededTransitionTrace>,
}

// ---------------------------------------------------------------------------
// EpochState
// ---------------------------------------------------------------------------

/// Epoch state managed by `EpochGuard` (governance) and read by the kernel
/// for trace logging.
///
/// ## Snapshot strategy
/// COW: full clone (~8 bytes). Single scalar — negligible weight.
///
/// Refs: I-Gov-Epoch-Reject
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct EpochState {
    pub current_generation: u64,
}

// ---------------------------------------------------------------------------
// Stream tool accumulator
// ---------------------------------------------------------------------------

/// Transient accumulator for tool calls discovered during LLM streaming.
///
/// The kernel populates this as `ToolCallStart` / `ToolArgumentChunk`
/// events arrive. When `ToolCallDone` is received, the pending descriptors
/// are drained, passed through the `on_tool_calls` hook, sealed into
/// `ActiveToolCall`s, and stored in `session.active_tools`.
///
/// Raw argument bytes are accumulated separately in `pending_args_bytes`
/// to avoid UTF-8 corruption from per-byte `char` casting. Conversion to
/// `String` happens only at the `ToolCallDone` materialization boundary.
///
/// This type is transient (#[brioche(no_snapshot)]) — it does not need
/// COW rollback because it is reconstructed on every stream event.
///
/// Refs: I-Core-ActiveToolCall, I-Core-ChunkBudget
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct StreamToolAccumulator {
    /// Map tool_id -> partially-built descriptor.
    pub pending: BTreeMap<String, ToolCallDescriptor>,
    /// Map tool_id -> accumulated raw argument bytes.
    /// Kept separate from `ToolCallDescriptor.arguments` so that split
    /// multi-byte UTF-8 characters at SSE chunk boundaries are preserved.
    #[brioche(deterministic_order)]
    pub pending_args_bytes: BTreeMap<String, Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Separate channels — Book III-A
// ---------------------------------------------------------------------------

/// System signals emitted by the shell and consumed by governance plugins
/// via adapters. These events do **not** transit through `EngineInput`.
///
/// Refs: SPECS.md §1.4, I-Shell-Network-Signal
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemSignal {
    /// Network failure detected at transport level.
    NetworkUnavailable { reason: String },
    /// User requested cancellation of the current operation.
    OperationCancelled,
    /// Periodic tick emitted by the shell for sub-routine timeout monitoring.
    Tick { elapsed_ms: u64 },
}

/// Result of an asynchronous task executed by the shell.
///
/// Consumed by governance plugins via `AsyncTaskResultAdapter`.
///
/// Refs: SPECS.md §1.4
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncTaskResult {
    /// Background summarization completed.
    SummarizationDone {
        summary: ChatMessage,
        watermark: u32,
    },
    /// CPU-intensive task completed.
    CpuTaskDone { task_id: String, result: Vec<u8> },
    /// Status check for a long-running (pending) tool task.
    ToolStatusCheck { task_id: String, status: ToolStatus },
}

/// Status of a pending tool task.
///
/// Refs: SPECS.md §1.4
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStatus {
    Running,
    Completed(ToolOutcome),
}

/// Governance notifications emitted by the shell.
///
/// Consumed by governance plugins (e.g. `QuarantineManager`) via
/// `GovernanceNotificationAdapter`.
///
/// Refs: SPECS.md §1.4
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GovernanceNotification {
    /// A plugin emitted a fatal error. The shell notifies governance
    /// so that `QuarantineManager` can decide on follow-up.
    PluginFaulted {
        plugin_name: String,
        error: PluginError,
    },
}

// ---------------------------------------------------------------------------
// Signal drainage — Book III-A
// ---------------------------------------------------------------------------

/// Batch of drained signals from the separate event channels.
///
/// Produced by `SignalDrainOrder::drain()` and consumed by the shell
/// to inject pending signals into `ExtensionStorage` before each
/// `transition()` cycle.
///
/// Canonical order is enforced by the `SignalDrainOrder` implementation:
/// `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`.
///
/// Refs: SPECS.md §1.4, I-Shell-Drain-Atomic
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalDrainBatch {
    /// Drained system signals (produced first in canonical order).
    pub system_signals: Vec<SystemSignal>,
    /// Drained governance notifications (produced second).
    pub governance_notifications: Vec<GovernanceNotification>,
    /// Drained async task results (produced third).
    pub async_task_results: Vec<AsyncTaskResult>,
}

/// Transient buffer holding drained signals for plugin consumption.
///
/// The shell inserts this into `ExtensionStorage` before each
/// `transition()` cycle. Plugins read from it in their hooks.
/// It is cleared and repopulated each cycle.
///
/// Marked `#[brioche(no_snapshot)]` because it is fully reconstructed
/// each cycle; rollback of this buffer is meaningless.
///
/// Refs: I-Shell-Drain-Atomic
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct SignalBuffer {
    #[brioche(deterministic_order)]
    pub system_signals: Vec<SystemSignal>,
    #[brioche(deterministic_order)]
    pub governance_notifications: Vec<GovernanceNotification>,
    #[brioche(deterministic_order)]
    pub async_task_results: Vec<AsyncTaskResult>,
}
