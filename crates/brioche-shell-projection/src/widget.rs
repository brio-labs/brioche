//! Widget type constants — Book III-C §1.2
//!
//! Special governance widgets emitted by kernel plugins via
//! `Effect::ForwardToUi`. These identifiers have predefined semantics
//! in the frontend and are pre-registered in `UiRegistry`.
//!
//! ## Invariants upheld
//! - I-UI-NoUIType: These are plain strings, not frontend component types.
//! - I-UI-NoDirectDOM: The frontend decides how to render each identifier.
//!
//! Refs: SPECS.md §Book III-C Ch 1.2

/// Warning banner displayed when a plugin has been quarantined.
///
/// Emitter: `QuarantineManager`
pub const WIDGET_SYSTEM_DEGRADED: &str = "system_degraded";

/// Displayed when a `SystemSignal::NetworkUnavailable` is intercepted.
///
/// Emitter: `RecoveryPolicy`
pub const WIDGET_NETWORK_ERROR: &str = "network_error";

/// Generic state widget (e.g. "cancelled").
///
/// Emitter: `RecoveryPolicy`
pub const WIDGET_STATUS: &str = "status";

/// Generic widget for errors (`Effect::Error` transformed by the shell).
///
/// Emitter: Shell runtime
pub const WIDGET_ERROR: &str = "error";

/// Displayed when a sub-routine exceeds its time limit.
///
/// Emitter: `SubRoutineTimeoutPolicy`
pub const WIDGET_SUBROUTINE_TIMEOUT: &str = "subroutine_timeout";

/// Text chunk emitted during LLM streaming.
///
/// This is the primary content widget. It is never dropped by the
/// `UiComposer` and always flushed with absolute priority.
///
/// Emitter: Shell runtime (from `LlmStream` events)
pub const WIDGET_TEXT_CHUNK: &str = "text_chunk";

/// Pending task status widget for long-running tool calls.
///
/// Emitter: `PendingTaskManager`
pub const WIDGET_PENDING_TASK: &str = "pending_task";
