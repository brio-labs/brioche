//! # Brioche Standard Plugins — Book IV
//!
//! Reference plugin implementations and ecosystem utilities.
//! All types here are policy, not mechanism.
//!
//! ## Public interface
//! - Standard governance plugins (circuit breaker, token tracker, etc.).
//! - `brioche_std` prelude for plugin authors.
//!
//! ## Invariants upheld
//! - I-Eco-ExtensionOverMod: Plugins extend via traits, never modify Core.
//! - I-Eco-OrderedCollections: All persisted state uses ordered collections.
//!
//! Refs: docs/SPECS.md §Book IV Ch 1

#![deny(clippy::unwrap_used, clippy::expect_used)]

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

pub mod audit_logger;
pub mod circuit_breaker;
pub mod gc_policy;
pub mod pending_task_manager;
pub mod token_tracker;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------
pub use audit_logger::{AuditEntry, AuditLogger, AuditLoggerState};
// Re-export standard timeout policy from governance-default.
//
// Refs: I-Eco-ExtensionOverMod
pub use brioche_governance_default::ToolTimeoutPolicy;
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerState};
pub use gc_policy::{ContextOptimizer, ContextOptimizerState, GcPolicy, GcPolicyState};
pub use pending_task_manager::{
    PendingTaskInfo, PendingTaskManager, PendingTaskState, PendingTaskStatus,
};
pub use token_tracker::{TokenRole, TokenTracker, TokenTrackerState};

// ---------------------------------------------------------------------------
// Priority constants
// ---------------------------------------------------------------------------

/// Named evaluation priorities for standard ecosystem plugins.
///
/// Lower values run earlier. Ties are broken lexicographically by plugin
/// `name`. Constants that overlap with `brioche_governance_default::Priority`
/// are documented with the tie-breaking rule.
///
/// Refs: I-Core-PluginOrder
pub struct Priority;

impl Priority {
    /// Very early input logger — record before interceptors block.
    ///
    /// Ties with `brioche_governance_default::Priority::QUARANTINE`;
    /// lexicographic order (`audit_logger` < `quarantine_manager`) wins.
    pub const AUDIT_LOGGER: i16 = -100;
    /// Early `before_prediction` guard — break tool-call loops.
    pub const CIRCUIT_BREAKER: i16 = -20;
    /// Context optimization just before prediction.
    pub const CONTEXT_OPTIMIZER: i16 = -5;
    /// Late `after_prediction` GC trigger.
    pub const GC_OBSERVER: i16 = 200;
    /// Tool-result pending-task detection.
    ///
    /// Ties with `brioche_governance_default::Priority::ARGUMENT_ACCUMULATOR`;
    /// lexicographic order (`json_argument_accumulator` < `pending_task_manager`) wins.
    pub const PENDING_TASK: i16 = 20;
    /// Late `before_prediction` token estimator.
    pub const TOKEN_TRACKER: i16 = 60;
}
