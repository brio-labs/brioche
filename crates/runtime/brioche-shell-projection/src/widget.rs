//! Widget type constants — Book III-C §1.2
//!
//! Canonical string identifiers for `UiWidget` variants used by the
//! projection layer and frontend registry. The kernel emits structured
//! `Effect::ForwardToUi(UiWidget)` effects; the projection layer maps
//! variants to these strings via `UiWidget::widget_type()` for
//! frontend resolution. Third-party widgets use `UiWidget::Custom`.
//!
//! ## Invariants upheld
//! - I-UI-NoUIType: These are plain strings, not frontend component types.
//! - I-UI-NoDirectDOM: The frontend decides how to render each identifier.
//!
//! Refs: SPECS.md §Book III-C Ch 1.2

/// Warning banner displayed when a plugin has been quarantined.
///
/// Emitter: `QuarantineManager`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_SYSTEM_DEGRADED: &str = "system_degraded";

/// Displayed when a `SystemSignal::NetworkUnavailable` is intercepted.
///
/// Emitter: `RecoveryPolicy`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_NETWORK_ERROR: &str = "network_error";

/// Generic state widget (e.g. "cancelled").
///
/// Emitter: `RecoveryPolicy`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_STATUS: &str = "status";

/// Generic widget for errors (`Effect::Error` transformed by the shell).
///
/// Emitter: Shell runtime
///
/// Refs: I-UI-NoUIType
pub const WIDGET_ERROR: &str = "error";

/// Displayed when a sub-routine exceeds its time limit.
///
/// Emitter: `SubRoutineTimeoutPolicy`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_SUBROUTINE_TIMEOUT: &str = "subroutine_timeout";

/// Text chunk emitted during LLM streaming.
///
/// This is the primary content widget. It is never dropped by the
/// `UiComposer` and always flushed with absolute priority.
///
/// Emitter: Shell runtime (from `LlmStream` events)
///
/// Refs: I-UI-NoUIType
pub const WIDGET_TEXT_CHUNK: &str = "text_chunk";

/// Displayed when a sub-routine has been successfully restored.
///
/// Emitter: Shell runtime (on `Effect::SubRoutineRestored`)
///
/// Refs: I-UI-NoUIType
pub const WIDGET_SUBROUTINE_LOADED: &str = "subroutine_loaded";

/// Pending task status widget for long-running tool calls.
///
/// Emitter: `PendingTaskManager`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_PENDING_TASK: &str = "pending_task";
