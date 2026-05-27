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
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
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
pub fn seal(descriptors: Vec<ToolCallDescriptor>) -> Vec<ActiveToolCall> {
    descriptors
        .into_iter()
        .map(|d| ActiveToolCall {
            tool_id: d.tool_id,
            tool_name: d.tool_name,
            arguments: d.arguments,
            timeout_ms: d.timeout_ms.unwrap_or(0),
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolOutcome {
    Success(String),
    BusinessError(String),
    SystemError(String),
    TimeoutWithPartialData { partial_output: Option<String> },
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
/// Refs: I-Core-Pure, I-Core-NoPanic, I-Shell-Session-NoSend
pub struct Session {
    pub id: String,
    pub history: Vec<ChatMessage>,
    /// Disk synchronization index for the Delta protocol (Redb).
    pub persisted_msg_count: usize,
    pub state: AgentState,
    pub state_stack: Vec<AgentState>,
    pub extensions: ExtensionStorage,
    /// Mechanical state: tools currently in execution.
    /// Managed exclusively by the kernel. Not modifiable by plugins.
    pub active_tools: Vec<ActiveToolCall>,
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
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Create a new session in `AgentState::Idle`.
    ///
    /// Complexity: O(1). Allocates empty collections.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            history: Vec::new(),
            persisted_msg_count: 0,
            state: AgentState::Idle,
            state_stack: Vec::new(),
            extensions: ExtensionStorage::new(),
            active_tools: Vec::new(),
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
    ///
    /// Refs: I-Core-Pure
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            current_state: AgentStateTag::from(&self.state),
            state_stack_depth: self.state_stack.len(),
        }
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
    /// Complexity: O(log n) where n = number of sub-routines.
    pub fn insert(&mut self, handle: SubRoutineHandle, session: Session) {
        self.sessions.insert(handle, session);
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
/// Refs: I-Core-Pure
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
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
    /// Request to hydrate a sub-routine into the `SessionRegistry`.
    RestoreSubRoutine {
        handle: SubRoutineHandle,
        head_blob: Vec<u8>,
    },
}

// ---------------------------------------------------------------------------
// PolicyDecision
// ---------------------------------------------------------------------------

/// Decision returned by a plugin hook, interpreted by the kernel.
///
/// Refs: I-Gov-Decision-Required, I-Gov-OverrideTrace
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryEdit {
    Insert { index: usize, message: ChatMessage },
    Replace { index: usize, message: ChatMessage },
    Truncate { keep_last: usize },
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    CallLlmNetwork,
    ExecuteTools(Vec<ActiveToolCall>),
    ForwardToUi {
        widget_type: String,
        payload: serde_json::Value,
    },
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
/// Refs: I-Core-ChunkBudget
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPath {
    pub nodes: Vec<String>,
}

/// Stream event delivered by the LLM provider.
///
/// `Bytes` is used for text fragments to avoid heap allocations in the
/// synchronous hot path. SSE payloads are pre-segmented to `MAX_INLINE_CHUNK`
/// (4096 bytes) by the shell.
///
/// Refs: I-Core-ChunkBudget, I-Core-StreamNoBranch
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
    Pass,
}

/// Action requested by a plugin in response to a stream event.
///
/// Refs: I-Core-StreamNoBranch
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
        Effect::ForwardToUi { .. } => EffectBit::FORWARD_TO_UI,
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
/// Refs: I-Gov-OverrideTrace, I-Core-NoPanic
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct TransitionTraceLog {
    #[brioche(deterministic_order)]
    pub entries: Vec<TransitionTrace>,
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
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct SupersededTransitionTraceLog {
    #[brioche(deterministic_order)]
    pub entries: Vec<SupersededTransitionTrace>,
}

// ---------------------------------------------------------------------------
// EpochState
// ---------------------------------------------------------------------------

/// Epoch state managed by `EpochGuard` (governance) and read by the kernel
/// for trace logging.
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
/// This type is transient (#[brioche(no_snapshot)]) — it does not need
/// COW rollback because it is reconstructed on every stream event.
///
/// Refs: I-Core-ActiveToolCall, I-Core-ChunkBudget
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
/// Refs: SPECS.md §1.4, I-Shell-Network-Signal
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
