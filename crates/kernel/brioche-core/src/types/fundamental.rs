//! Fundamental kernel types.
//!
//! Strongly-typed identifiers and error types used across all other
//! kernel modules. These are the atomic building blocks of the type system.

use serde::{Deserialize, Serialize};

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
    /// Create a new handle. O(1). Never panics.
    /// Refs: I-Core-AgentState
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the underlying string.
    /// Refs: I-Core-AgentState
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
    /// Refs: I-Core-PluginOrder
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
// Errors
// ---------------------------------------------------------------------------

/// Policy error emitted by plugins.
///
/// - `Soft`: minor error. Logged; evaluation continues.
/// - `Fatal`: structural error. The kernel emits `Effect::PluginFault`.
///
/// Refs: docs/SPECS.md §1.5
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
/// Refs: I-Core-NoPanic, docs/SPECS.md §1.5
/// # Complexity
/// O(1) for construction and field/variant access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[non_exhaustive]
pub enum BriocheError {
    #[error("invalid state transition: {0}")]
    /// Transition violates the automaton rules.
    InvalidStateTransition(String),
    #[error("history index out of bounds: {operation} at {index} (len={len})")]
    /// History edit index out of bounds.
    HistoryIndexOutOfBounds {
        /// Which operation failed.
        operation: crate::types::effect::HistoryOperation,
        /// The index that was out of bounds.
        index: usize,
        /// Current history length.
        len: usize,
    },
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
