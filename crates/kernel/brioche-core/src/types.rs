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

use std::collections::BTreeMap;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::BriocheExtensionType;
use crate::extension::ExtensionStorage;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default tool timeout applied when a descriptor omits `timeout_ms`.
///
/// The kernel materializes this value during `seal()` when no plugin
/// has set a timeout on the `ToolCallDescriptor`.
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
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SubRoutineHandle(String);

impl SubRoutineHandle {
    /// Create a new handle from any string-like value.
    ///
    /// Complexity: O(length of id). Allocates one `String`.
    ///
    /// Refs: I-Core-AgentState
    /// # Panics
    /// Never panics.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the underlying string.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Core-AgentState
    /// # Panics
    /// Never panics.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Strong identifiers
// ---------------------------------------------------------------------------

/// Strongly-typed identifier for the plugin that produced a decision,
/// owned a blob, or faulted.
///
/// Replaces bare `String` plugin names in `Effect` and `InputResult` so
/// that the compiler rejects accidental mixing with arbitrary strings or
/// other identifiers (e.g., `TaskId`).
///
/// # Complexity
/// O(1) copy of the inner `String` reference. Clones allocate.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-PluginOrder
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSource(pub String);

impl PluginSource {
    /// Borrow the underlying plugin name.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Core-PluginOrder
    /// # Panics
    /// Never panics.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for PluginSource {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for PluginSource {
    fn from(value: String) -> Self {
        Self(value)
    }
}

/// Strongly-typed identifier for an offloaded CPU task.
///
/// Replaces bare `String` task IDs in `Effect::ExecuteCpuTask` so that
/// the compiler rejects accidental mixing with `PluginSource` or other
/// string-like identifiers.
///
/// # Complexity
/// O(1) copy of the inner `String` reference. Clones allocate.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-RetVecEffect
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl From<&str> for TaskId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for TaskId {
    fn from(value: String) -> Self {
        Self(value)
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
// Tool descriptors
// ---------------------------------------------------------------------------

/// Tool call descriptor — the plugin-facing interface for tool calls.
///
/// Plugins inspect and mutate `ToolCallDescriptor` via the `on_tool_calls`
/// hook. The kernel converts these into `ActiveToolCall` via `seal()`.
///
/// Refs: I-Core-ActiveToolCall
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallDescriptor {
    /// Opaque identifier assigned by the LLM provider.
    pub tool_id: String,
    /// Name of the tool, as registered in the tool registry.
    pub tool_name: String,
    /// JSON-encoded arguments for the tool call.
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
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveToolCall {
    /// Opaque identifier assigned by the LLM provider.
    pub tool_id: String,
    /// Name of the tool, as registered in the tool registry.
    pub tool_name: String,
    /// JSON-encoded arguments for the tool call.
    pub arguments: String,
    /// Materialized by the kernel after `on_tool_calls` hook execution.
    pub timeout_ms: u64,
}

/// Canonical conversion from a single `ToolCallDescriptor` to `ActiveToolCall`.
///
/// Extracted as a pure function so the compiler forces exhaustive field
/// mapping without `Vec` allocation overhead in hot paths.
///
/// `default_timeout_ms` is applied when `descriptor.timeout_ms` is `None`.
/// This ensures every `ActiveToolCall` has a concrete timeout — never zero
/// unless explicitly requested by the descriptor.
///
/// Complexity: O(1). No heap allocation.
///
/// Refs: I-Core-ActiveToolCall
/// # Panics
/// Never panics.
pub fn seal_single(descriptor: ToolCallDescriptor, default_timeout_ms: u64) -> ActiveToolCall {
    ActiveToolCall {
        tool_id: descriptor.tool_id,
        tool_name: descriptor.tool_name,
        arguments: descriptor.arguments,
        timeout_ms: descriptor.timeout_ms.unwrap_or(default_timeout_ms),
    }
}

/// Canonical conversion from interface type to mechanical type.
///
/// Called immediately after `handle_tool_calls`. Any new field must be mapped
/// explicitly here; the Rust compiler forces exhaustive matching.
///
/// `default_timeout_ms` is applied to any descriptor lacking an explicit
/// timeout. Use the engine's configured `default_tool_timeout_ms()` to
/// preserve consistency with the main dispatch path.
///
/// Complexity: O(n) where n = number of descriptors. Allocates one `Vec`.
///
/// Refs: I-Core-ActiveToolCall
/// # Panics
/// Never panics.
pub fn seal(descriptors: Vec<ToolCallDescriptor>, default_timeout_ms: u64) -> Vec<ActiveToolCall> {
    descriptors
        .into_iter()
        .map(|d| seal_single(d, default_timeout_ms))
        .collect()
}

/// Convert a `ToolOutcome` into its string representation for history injection.
///
/// This is a pure function extracted from both the kernel's
/// `dispatch_tool_calls_result` and `SubRoutineOrchestrator` to eliminate
/// duplication and keep mechanism code minimal.
///
/// Complexity: O(1). May clone an inner `String`.
///
/// Refs: I-Comp-Pure-Logic
/// # Panics
/// Never panics.
pub fn tool_outcome_to_string(outcome: &ToolOutcome) -> String {
    match outcome {
        ToolOutcome::Success(s) | ToolOutcome::BusinessError(s) | ToolOutcome::SystemError(s) => {
            s.clone()
        }
        ToolOutcome::TimeoutWithPartialData { partial_output } => {
            partial_output.clone().unwrap_or_default()
        }
    }
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
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolOutcome {
    /// Tool completed successfully. Result injected into history.
    Success(String),
    /// Domain-level error. The LLM may retry.
    BusinessError(String),
    /// Tool crashed or was unreachable.
    SystemError(String),
    /// Tool exceeded its timeout. Partial output may be available.
    TimeoutWithPartialData {
        /// Partial output.
        partial_output: Option<String>,
    },
}

/// Structured result returned from the shell to the kernel after tool execution.
///
/// Refs: I-Core-Pure
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultDTO {
    /// Opaque identifier assigned by the LLM provider.
    pub tool_id: String,
    /// Name of the tool, as registered in the tool registry.
    pub tool_name: String,
    /// Execution outcome: success, business error, system error, or timeout.
    pub outcome: ToolOutcome,
}

/// Structured truncation metadata for oversized tool results.
///
/// Replaces hand-rolled JSON `format!()` with a typed domain object
/// that serializes deterministically via `serde_json`.
///
/// Refs: I-Comp-Pure-Logic, I-Comp-Typed-Effects
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TruncatedToolResult {
    /// Whether the result was truncated due to size limits.
    pub truncated: bool,
    /// Original byte length before truncation.
    pub original_len: usize,
    /// First `max_bytes` of the original content.
    pub preview: String,
}

impl TruncatedToolResult {
    /// Creates a truncation record from the full content and a byte limit.
    ///
    /// Complexity: O(1). One `String` allocation for the preview.
    ///
    /// Refs: I-Comp-Pure-Logic
    /// # Panics
    /// Never panics.
    pub fn from_content(content: &str, max_bytes: usize) -> Self {
        let preview = content[..max_bytes.min(content.len())].to_string();
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
    ///
    /// Refs: I-Comp-Pure-Logic
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
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Gov-Rollback-BestEffort
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
/// # Complexity
/// O(1) for construction and field/variant access.
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
    /// Complexity: O(1). Allocates empty collections.
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

    /// Produce a read-only `SessionSnapshot` for plugin consumption.
    ///
    /// The kernel injects this into `ExtensionStorage` before each hook
    /// so that plugins can read session state without direct field access.
    ///
    /// Complexity: O(1). No allocation.
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
    /// Complexity: O(1). One `Vec` push if text exists.
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
    /// Complexity: O(e) where e = number of edits. One `Vec` insert/replace
    /// per edit, plus one `drain` for `Truncate`.
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
                        return Err(BriocheError::InvalidStateTransition(format!(
                            "history insert index {} out of bounds (len={})",
                            index,
                            self.history.len()
                        )));
                    }
                    self.history.insert(*index, message.clone());
                }
                HistoryEdit::Replace { index, message } => {
                    if *index >= self.history.len() {
                        return Err(BriocheError::InvalidStateTransition(format!(
                            "history replace index {} out of bounds (len={})",
                            index,
                            self.history.len()
                        )));
                    }
                    self.history[*index] = message.clone();
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
    /// Complexity: O(1).
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
    /// Complexity: O(log n) where n = number of sub-routines.
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn insert(&mut self, handle: SubRoutineHandle, session: Session) {
        self.sessions.insert(handle, session);
    }

    /// Get a mutable reference to a sub-routine session.
    ///
    /// Complexity: O(log n).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn get_mut(&mut self, handle: &SubRoutineHandle) -> Option<&mut Session> {
        self.sessions.get_mut(handle)
    }

    /// Remove a sub-routine session, returning it if present.
    ///
    /// Complexity: O(log n).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn remove(&mut self, handle: &SubRoutineHandle) -> Option<Session> {
        self.sessions.remove(handle)
    }

    /// Returns `true` if the registry contains the given handle.
    ///
    /// Complexity: O(log n).
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
    /// Complexity: O(log n).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn increment_exit_count(&mut self, handle: &SubRoutineHandle) {
        *self.exit_counts.entry(handle.clone()).or_insert(0) += 1;
    }

    /// Get the current exit count for a handle.
    ///
    /// Complexity: O(log n).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn get_exit_count(&self, handle: &SubRoutineHandle) -> u64 {
        self.exit_counts.get(handle).copied().unwrap_or(0)
    }

    /// Iterate over all registered handles.
    ///
    /// Complexity: O(1) for the iterator creation.
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
    /// Complexity: O(1). One pattern match.
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
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EngineInput {
    /// User message. Triggers `Idle -> Predicting` transition.
    UserMessage(String),
    /// LLM stream fragments.
    LlmStream(StreamEvent),
    /// Tool execution results (parallelized by the shell).
    ToolCallsResult {
        /// Must match the current epoch or the result is rejected.
        generation_id: u64,
        /// Parallel tool execution outcomes.
        results: Vec<ToolResultDTO>,
    },
    /// Request to hydrate a sub-routine into the `SessionRegistry`.
    RestoreSubRoutine {
        /// Handle for the restored sub-routine session.
        handle: SubRoutineHandle,
        /// Serialized session head (postcard-encoded `SessionHeadDTO`).
        head_blob: Vec<u8>,
    },
}

// ---------------------------------------------------------------------------
// PolicyDecision
// ---------------------------------------------------------------------------

/// Decision returned by a plugin hook, interpreted by the kernel.
///
/// Refs: I-Gov-Decision-Required, I-Gov-OverrideTrace
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum PolicyDecision {
    /// Allow the current operation to proceed.
    Allow,
    /// Block the current operation with a reason.
    Block {
        /// Human-readable explanation for the block.
        reason: String,
    },
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
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HistoryEdit {
    /// Insert a message at a specific history index.
    Insert {
        /// Position in history for the edit operation.
        index: usize,
        /// The `ChatMessage` to insert or replace.
        message: ChatMessage,
    },
    /// Overwrite a message at a specific history index.
    Replace {
        /// Position in history for the edit operation.
        index: usize,
        /// The `ChatMessage` to insert or replace.
        message: ChatMessage,
    },
    /// Discard all but the most recent N messages.
    Truncate {
        /// Number of most recent messages to retain.
        keep_last: usize,
    },
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
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum UiWidget {
    /// Text fragment from LLM streaming.
    TextChunk {
        /// Correlation ID for the current LLM stream.
        trace_id: String,
        /// Fragment of generated text.
        text: String,
    },
    /// Generic error notification displayed in the content area.
    Error {
        /// Error code for classification and retry logic.
        code: String,
        /// Human-readable error description.
        message: String,
    },
    /// Critical system error (e.g., governance cascade failure).
    CriticalError {
        /// Name of the subsystem that failed.
        component: String,
        /// Optional technical detail for debugging.
        detail: Option<String>,
    },
    /// System degradation banner (e.g., plugin quarantined).
    SystemDegraded {
        /// Name of the quarantined or failing plugin.
        plugin: String,
    },
    /// Network unavailability notification.
    NetworkError {
        /// Transport-level failure description.
        reason: String,
    },
    /// Generic status indicator (e.g., "cancelled").
    Status(String),
    /// Sub-routine timeout notification.
    SubRoutineTimeout {
        /// Handle for the restored sub-routine session.
        handle: SubRoutineHandle,
        /// Timeout limit that was exceeded.
        limit_ms: u64,
    },
    /// Sub-routine successfully restored.
    SubRoutineLoaded {
        /// Handle of the restored sub-routine.
        handle: SubRoutineHandle,
    },
    /// Pending task status update.
    PendingTask {
        /// Identifier of the background task.
        task_id: String,
        /// Current status string (e.g., "running", "completed").
        status: String,
    },
    /// Test widget for integration tests.
    Test {
        /// Test message payload.
        msg: String,
    },
    /// Catch-all for unknown third-party widgets.
    ///
    /// Payload is raw JSON bytes to preserve determinism.
    /// The projection layer deserializes on the shell side.
    ///
    /// Refs: I-Comp-Typed-Effects
    Custom {
        /// Canonical type string for third-party widget routing.
        widget_type: String,
        /// Raw JSON payload. Deterministic because it is bytes.
        payload_json: Vec<u8>,
    },
}

impl UiWidget {
    /// Returns the canonical widget type string.
    ///
    /// Used by the projection layer for registry lookup and priority
    /// classification while the ecosystem migrates to structured variants.
    ///
    /// Complexity: O(1).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Comp-Typed-Effects
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

/// Structured error payload for `Effect::Error`.
///
/// Replaces the previous `message: String` anti-pattern with typed,
/// exhaustively matchable variants. The shell and projection layer can
/// inspect specific error scenarios without string parsing.
///
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
/// Refs: I-Comp-Typed-Effects
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorDetail {
    /// Fallback for errors that do not yet have a structured variant.
    Generic(String),
    /// History edit index out of bounds.
    HistoryIndexOutOfBounds {
        /// Which edit failed: insert, replace, or truncate.
        operation: String,
        /// Position in history for the edit operation.
        index: usize,
        /// Current history length at the time of the failed edit.
        len: usize,
    },
    /// Tool descriptor missing timeout (default applied).
    MissingToolTimeout {
        /// Default timeout applied when the descriptor omits one.
        default_timeout_ms: u64,
    },
    /// Effect variant not allowed on the current hook.
    EffectNotAllowed {
        /// Name of the hook on which the effect was requested.
        hook: String,
        /// Name of the disallowed `Effect` variant.
        effect_variant: String,
    },
    /// Effects were dropped after `RebuildRoutes`.
    EffectsDroppedAfterRebuildRoutes {
        /// Number of discarded effects.
        count: usize,
    },
    /// Sub-routine lifecycle guard failed.
    SubRoutineLifecycleFailed {
        /// Name of the lifecycle guard that failed.
        guard_name: String,
    },
    /// State inconsistency detected by a governance plugin or internal check.
    StateInconsistent {
        /// Source of the inconsistency (plugin name or internal module).
        source: String,
    },
}

impl std::fmt::Display for ErrorDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorDetail::Generic(s) => write!(f, "{}", s),
            ErrorDetail::HistoryIndexOutOfBounds {
                operation,
                index,
                len,
            } => {
                write!(
                    f,
                    "history {} index {} out of bounds (len={})",
                    operation, index, len
                )
            }
            ErrorDetail::MissingToolTimeout { default_timeout_ms } => {
                write!(
                    f,
                    "Missing timeout, applied default {} ms",
                    default_timeout_ms
                )
            }
            ErrorDetail::EffectNotAllowed {
                hook,
                effect_variant,
            } => {
                write!(f, "Effect {} not allowed on hook {}", effect_variant, hook)
            }
            ErrorDetail::EffectsDroppedAfterRebuildRoutes { count } => {
                write!(f, "{} effect(s) dropped after RebuildRoutes", count)
            }
            ErrorDetail::SubRoutineLifecycleFailed { guard_name } => {
                write!(f, "Sub-routine lifecycle guard '{}' failed", guard_name)
            }
            ErrorDetail::StateInconsistent { source } => {
                write!(f, "State inconsistent: {}", source)
            }
        }
    }
}

/// Declarative effect emitted by the kernel. The shell is responsible for
/// execution.
///
/// `Effect` contains **only** pure mechanical effects. No telemetry,
/// UI fallback, or specific notification variants appear here.
///
/// Refs: I-Core-EffectPure, I-Core-RetVecEffect
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Effect {
    /// Request the shell to initiate an LLM prediction.
    CallLlmNetwork,
    /// Request the shell to execute active tool calls.
    ExecuteTools(Vec<ActiveToolCall>),
    /// Emit a structured widget to the projection layer.
    ForwardToUi(UiWidget),
    /// Report a system-level error. The shell decides on recovery.
    Error {
        /// Error code for classification and retry logic.
        code: ErrorCode,
        /// Optional technical detail for debugging.
        detail: ErrorDetail,
    },
    /// Persist the current session head to disk (Delta protocol).
    SaveSession,
    /// Persist a plugin-specific binary blob.
    SavePluginBlob {
        /// Plugin that owns this blob.
        plugin_id: PluginSource,
        /// Opaque binary payload. Serialized by the plugin itself.
        data: Vec<u8>,
    },
    /// Start a background summarization task.
    TriggerSummarization,
    /// Offload a CPU-intensive computation to the shell.
    ExecuteCpuTask {
        /// Identifier of the background task.
        task_id: TaskId,
        /// Serialized input for the offloaded computation.
        payload: Vec<u8>,
    },
    /// Request garbage collection of orphaned sub-routines.
    TriggerGc,
    /// Notify the shell that the kernel is idle and awaiting input.
    SystemIdle,
    /// A plugin fatally errored. Triggers quarantine evaluation.
    PluginFault {
        /// Plugin that faulted.
        plugin_name: PluginSource,
        /// The fatal error that triggered this notification.
        error: PluginError,
    },
    /// Rebuild the plugin routing table (after quarantine).
    RebuildRoutes,
    /// A sub-routine was successfully hydrated from disk.
    SubRoutineRestored {
        /// Handle for the restored sub-routine session.
        handle: SubRoutineHandle,
    },
}

/// Mechanical error codes carried by `Effect::Error`.
///
/// These are **not** plugin errors; they represent system-level conditions
/// that the shell must handle.
///
/// Refs: I-Core-NoPanic
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Transport-level network failure.
    NetworkUnavailable,
    /// User cancelled the current operation.
    OperationCancelled,
    /// Internal state violates an invariant.
    StateInconsistency,
    /// Async response carries a stale generation ID.
    EpochMismatch,
    /// A governance plugin crashed or returned a fatal error.
    PluginFaulted,
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

/// Execution path for nested / tree-structured stream events.
///
/// # Panics
/// Never panics.
/// Refs: I-Core-ChunkBudget
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPath {
    /// Ordered list of nested node identifiers for tree-structured output.
    pub nodes: Vec<String>,
}

/// Stream event delivered by the LLM provider.
///
/// `Bytes` is used for text fragments to avoid heap allocations in the
/// synchronous hot path. SSE payloads are pre-segmented to `MAX_INLINE_CHUNK`
/// (4096 bytes) by the shell.
///
/// Refs: I-Core-ChunkBudget, I-Core-StreamNoBranch
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StreamEvent {
    /// Fragment of LLM-generated text.
    TextChunk {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
        /// Text or argument fragment (pre-segmented to ≤ 4 KB).
        chunk: Bytes,
    },
    /// Beginning of a tool call declaration in the stream.
    ToolCallStart {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
        /// Stable identifier for the tool call or result.
        id: String,
        /// Name of the tool being invoked.
        name: String,
    },
    /// Fragment of tool call arguments (JSON).
    ToolArgumentChunk {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
        /// Stable identifier for the tool call or result.
        id: String,
        /// Text or argument fragment (pre-segmented to ≤ 4 KB).
        chunk: Bytes,
    },
    /// End of a tool call declaration.
    ToolCallDone {
        /// Nested execution path for tree-structured models.
        path: ExecutionPath,
    },
    /// End-of-stream marker. Sent by the shell when the LLM response
    /// completes without further chunks or tool calls.
    Done,
    /// No-op event. Used for heartbeat / keepalive.
    Pass,
}

/// Action requested by a plugin in response to a stream event.
///
/// Refs: I-Core-StreamNoBranch
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StreamAction {
    /// Let the chunk pass through.
    Pass,
    /// Hold the chunk (buffering).
    Hold,
    /// Offload a CPU-intensive task to the shell.
    OffloadTask {
        /// Identifier of the background task.
        task_id: String,
        /// Serialized input for the offloaded computation.
        payload: Vec<u8>,
    },
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
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[non_exhaustive]
pub enum PluginError {
    #[error("soft error in plugin {plugin_name}: {message}")]
    /// Non-fatal error. Logged; evaluation continues.
    Soft {
        /// Plugin name.
        plugin_name: String,
        /// Human-readable error message.
        message: String,
    },
    #[error("fatal error in plugin {plugin_name}: {message}")]
    /// Structural error. The kernel emits `Effect::PluginFault`.
    Fatal {
        /// Plugin name.
        plugin_name: String,
        /// Human-readable error message.
        message: String,
    },
}

/// System error — internal monolith failure.
///
/// These are never panics; they are returned as `Result::Err` and
/// typically converted into `Effect::Error` or `AgentState::Failure`.
///
/// Refs: I-Core-NoPanic, SPECS.md §1.5
/// # Complexity
/// O(1) for construction and field/variant access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[non_exhaustive]
pub enum BriocheError {
    #[error("invalid state transition: {0}")]
    /// Transition violates the automaton rules.
    InvalidStateTransition(String),
    #[error("storage access failed: {0}")]
    /// ExtensionStorage lookup or mutation failed.
    StorageAccess(String),
    #[error("serialization failed: {0}")]
    /// Binary serialization/deserialization failed.
    Serialization(String),
    #[error("plugin not found: {0}")]
    /// Referenced plugin is not registered.
    PluginNotFound(String),
    #[error("other error: {0}")]
    /// Catch-all for unclassified system errors.
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
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EpochAction {
    /// Input is valid for the current epoch; proceed with standard dispatch.
    Proceed,
    /// Input belongs to a past epoch; reject silently.
    Block {
        /// Human-readable explanation for the epoch rejection.
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// EffectBit
// ---------------------------------------------------------------------------

/// Bitmask constants for each `Effect` variant, used by `HookEffectConstraint`
/// for O(1) permission validation.
/// # Panics
/// Never panics.
///
/// Refs: I-Core-HookEffect-O1
pub struct EffectBit;

impl EffectBit {
    /// Bit for `Effect::CallLlmNetwork`.
    pub const CALL_LLM_NETWORK: u64 = 1 << 0;
    /// Bit for `Effect::Error`.
    pub const ERROR: u64 = 1 << 3;
    /// Bit for `Effect::ExecuteCpuTask`.
    pub const EXECUTE_CPU_TASK: u64 = 1 << 7;
    /// Bit for `Effect::ExecuteTools`.
    pub const EXECUTE_TOOLS: u64 = 1 << 1;
    /// Bit for `Effect::ForwardToUi`.
    pub const FORWARD_TO_UI: u64 = 1 << 2;
    /// Bit for `Effect::PluginFault`.
    pub const PLUGIN_FAULT: u64 = 1 << 10;
    /// Bit for `Effect::RebuildRoutes`.
    pub const REBUILD_ROUTES: u64 = 1 << 11;
    /// Bit for `Effect::SavePluginBlob`.
    pub const SAVE_PLUGIN_BLOB: u64 = 1 << 5;
    /// Bit for `Effect::SaveSession`.
    pub const SAVE_SESSION: u64 = 1 << 4;
    /// Bit for `Effect::SubRoutineRestored`.
    pub const SUB_ROUTINE_RESTORED: u64 = 1 << 12;
    /// Bit for `Effect::SystemIdle`.
    pub const SYSTEM_IDLE: u64 = 1 << 9;
    /// Bit for `Effect::TriggerGc`.
    pub const TRIGGER_GC: u64 = 1 << 8;
    /// Bit for `Effect::TriggerSummarization`.
    pub const TRIGGER_SUMMARIZATION: u64 = 1 << 6;
    // Bits 13-63 reserved for future extensions.
}

/// Map an `Effect` to its bitmask constant.
///
/// Complexity: O(1). Match on enum variant.
/// # Panics
/// Never panics.
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
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionTrace {
    /// Name of the plugin that emitted the `OverrideTransition`.
    pub source_plugin: String,
    /// The actual decision that was applied.
    pub decision: PolicyDecision,
    /// Generation ID at the time of the override.
    pub epoch: u64,
}

/// Ring buffer for traceability of applied `OverrideTransition`s (max 128 entries, FIFO).
///
/// ## Snapshot strategy
/// COW: full clone. Weight ~O(n) where n = entries (max 128).
///
/// Refs: I-Gov-OverrideTrace, I-Core-NoPanic
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct TransitionTraceLog {
    #[brioche(deterministic_order)]
    /// Ring buffer of overrides (max 128, FIFO eviction).
    pub entries: Vec<TransitionTrace>,
}

impl TransitionTraceLog {
    const CAPACITY: usize = 128;

    /// Push a trace entry, evicting the oldest if at capacity.
    ///
    /// Complexity: O(n) in the worst case (vec shift at capacity),
    /// bounded by `CAPACITY` (128). Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn push(&mut self, entry: TransitionTrace) {
        if self.entries.len() >= Self::CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// Take all entries, leaving the log empty.
    ///
    /// Complexity: O(1).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn take_entries(&mut self) -> Vec<TransitionTrace> {
        std::mem::take(&mut self.entries)
    }
}

/// Single entry in the `SupersededTransitionTraceLog` ring buffer.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersededTransitionTrace {
    /// Name of the plugin that emitted the `OverrideTransition`.
    pub source_plugin: String,
    /// The decision that was overridden.
    pub attempted_decision: PolicyDecision,
    /// Name of the plugin whose override won.
    pub preempted_by: String,
    /// Generation ID at the time of the override.
    pub epoch: u64,
}

/// Ring buffer of preempted `OverrideTransition`s (max 128 entries, FIFO).
///
/// ## Snapshot strategy
/// COW: full clone. Weight ~O(n) where n = entries (max 128).
///
/// Refs: I-Gov-OverrideTrace
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct SupersededTransitionTraceLog {
    #[brioche(deterministic_order)]
    /// Ring buffer of overrides (max 128, FIFO eviction).
    pub entries: Vec<SupersededTransitionTrace>,
}

impl SupersededTransitionTraceLog {
    const CAPACITY: usize = 128;

    /// Push a trace entry, evicting the oldest if at capacity.
    ///
    /// Complexity: O(n) in the worst case (vec shift at capacity),
    /// bounded by `CAPACITY` (128). Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn push(&mut self, entry: SupersededTransitionTrace) {
        if self.entries.len() >= Self::CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// Take all entries, leaving the log empty.
    ///
    /// Complexity: O(1).
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn take_entries(&mut self) -> Vec<SupersededTransitionTrace> {
        std::mem::take(&mut self.entries)
    }
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
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct EpochState {
    /// Monotonically increasing generation counter.
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
/// This type is transient (#[brioche(no_snapshot)]) — it does not need
/// COW rollback because it is reconstructed on every stream event.
///
/// Refs: I-Core-ActiveToolCall, I-Core-ChunkBudget
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct StreamToolAccumulator {
    /// Map tool_id -> partially-built descriptor.
    pub pending: BTreeMap<String, ToolCallDescriptor>,
}

// ---------------------------------------------------------------------------
// Separate channels — Book III-A
// ---------------------------------------------------------------------------

/// System signals emitted by the shell and consumed by governance plugins
/// via adapters. These events do **not** transit through `EngineInput`.
///
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
/// Refs: SPECS.md §1.4, I-Shell-Network-Signal
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemSignal {
    /// Transport failure detected by the shell.
    NetworkUnavailable {
        /// Transport failure description.
        reason: String,
    },
    /// User requested cancellation of the current operation.
    OperationCancelled,
    /// Periodic heartbeat for timeout monitoring.
    Tick {
        /// Monotonically increasing milliseconds since session start.
        elapsed_ms: u64,
    },
}

/// Result of an asynchronous task executed by the shell.
///
/// Consumed by governance plugins via `AsyncTaskResultAdapter`.
///
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
/// Refs: SPECS.md §1.4
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncTaskResult {
    /// Background summarization completed.
    SummarizationDone {
        /// Compressed chat message for history truncation.
        summary: ChatMessage,
        /// History index up to which summarization is valid.
        watermark: u32,
    },
    /// Offloaded computation finished.
    CpuTaskDone {
        /// Identifier matching the original `Effect::ExecuteCpuTask`.
        task_id: String,
        /// Serialized output of the CPU task.
        result: Vec<u8>,
    },
    /// Status update for a pending tool task.
    ToolStatusCheck {
        /// Identifier of the pending tool.
        task_id: String,
        /// Current execution status.
        status: ToolStatus,
    },
}

/// Status of a pending tool task.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: SPECS.md §1.4
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool is still executing.
    Running,
    /// Tool finished (success or failure in `ToolOutcome`).
    Completed(ToolOutcome),
}

/// Governance notifications emitted by the shell.
///
/// Consumed by governance plugins (e.g. `QuarantineManager`) via
/// `GovernanceNotificationAdapter`.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: SPECS.md §1.4
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GovernanceNotification {
    /// A plugin emitted a fatal error. The shell notifies governance
    /// so that `QuarantineManager` can decide on follow-up.
    PluginFaulted {
        /// Plugin name.
        plugin_name: String,
        /// The fatal error that triggered this notification.
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
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
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
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct SignalBuffer {
    #[brioche(deterministic_order)]
    /// System-level events (network, cancel, tick) — produced first.
    pub system_signals: Vec<SystemSignal>,
    #[brioche(deterministic_order)]
    /// Plugin fault notifications — produced second.
    pub governance_notifications: Vec<GovernanceNotification>,
    #[brioche(deterministic_order)]
    /// Background task completions — produced third.
    pub async_task_results: Vec<AsyncTaskResult>,
}
