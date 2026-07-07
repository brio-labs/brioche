//! Tool call lifecycle types.
//!
//! Descriptors, outcomes, and result DTOs for the tool execution pipeline.

use serde::{Deserialize, Serialize};

use super::fundamental::BriocheError;
use crate::BriocheExtensionType;

// Tool descriptors
// ---------------------------------------------------------------------------

/// Tool call descriptor — the plugin-facing interface for tool calls.
///
/// Plugins inspect and mutate `ToolCallDescriptor` via the `on_tool_calls`
/// hook. The kernel converts these into `ActiveToolCall` via `seal()`.
///
/// ## Snapshot strategy
/// COW: full clone. Weight is three `String` fields plus one optional `u64`.
///
/// Refs: I-Core-ActiveToolCall
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
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
/// ## Snapshot strategy
/// COW: full clone. Weight is three `String` fields plus one `u64`.
///
/// Refs: I-Core-ActiveToolCall
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
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
/// # Complexity
/// O(1). No heap allocation.
///
/// Refs: I-Core-ActiveToolCall
/// # Panics
/// Never panics.
pub fn seal_single(descriptor: ToolCallDescriptor, default_timeout_ms: u64) -> ActiveToolCall {
    ActiveToolCall {
        tool_id: descriptor.tool_id,
        tool_name: descriptor.tool_name,
        arguments: descriptor.arguments,
        timeout_ms: match descriptor.timeout_ms {
            Some(t) => t,
            None => default_timeout_ms,
        },
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
/// # Complexity
/// O(n) where n = number of descriptors. Allocates one `Vec`.
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
/// # Complexity
/// O(1). May clone an inner `String`.
///
/// Refs: I-Comp-Pure-Logic
/// # Panics
/// Never panics.
pub fn tool_outcome_to_string(outcome: &ToolOutcome) -> String {
    match outcome {
        ToolOutcome::Success(s) | ToolOutcome::BusinessError(s) | ToolOutcome::SystemError(s) => {
            s.clone()
        }
        ToolOutcome::TimeoutWithPartialData {
            partial_output: Some(s),
        } => s.clone(),
        ToolOutcome::TimeoutWithPartialData {
            partial_output: None,
        } => String::new(),
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
/// Refs: docs/SPECS.md §1.5
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
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

impl Default for ToolOutcome {
    fn default() -> Self {
        Self::Success(String::new())
    }
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
    /// # Complexity
    /// O(1). One `String` allocation for the preview.
    ///
    /// Refs: I-Comp-Pure-Logic
    /// # Panics
    /// Never panics.
    pub fn from_content(content: &str, max_bytes: usize) -> Self {
        let limit = max_bytes.min(content.len());
        let preview = match content.get(..limit) {
            Some(s) => s.to_string(),
            None => String::new(),
        };
        Self {
            truncated: true,
            original_len: content.len(),
            preview,
        }
    }

    /// Serializes to a JSON string for injection into `ToolOutcome::Success`.
    ///
    /// # Complexity
    /// O(n) where n = JSON length. One `String` allocation.
    ///
    /// # Panics
    /// Never panics.
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
