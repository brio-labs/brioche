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
//! Refs: SPECS.md §Book IV

#![deny(clippy::unwrap_used, clippy::expect_used)]

// ---------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------

pub mod audit_logger;
pub mod circuit_breaker;
pub mod context_optimizer;
pub mod gc_policy;
pub mod pending_task_manager;
pub mod token_tracker;
pub mod tool_timeout_policy;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use audit_logger::{AuditEntry, AuditLogger, AuditLoggerState};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerState};
pub use context_optimizer::{ContextOptimizer, ContextOptimizerState};
pub use gc_policy::{GcPolicy, GcPolicyState};
pub use pending_task_manager::{
    PendingTaskInfo, PendingTaskManager, PendingTaskState, PendingTaskStatus,
};
pub use token_tracker::{TokenTracker, TokenTrackerState};
pub use tool_timeout_policy::{ToolTimeoutPolicy, ToolTimeoutState};
