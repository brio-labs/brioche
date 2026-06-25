//! # Brioche Shell Runtime — Book III-A
//!
//! Async runtime, networking, and system I/O. The shell is the only
//! layer permitted to perform side effects.
//!
//! ## Public interface
//! - [`BriocheShell`]: Main async runtime coordinator.
//! - [`SystemSignal`], [`AsyncTaskResult`], [`GovernanceNotification`]: Separate channel types.
//! - [`EffectExecutor`]: Dispatches `Effect` to async handlers.
//! - [`BackpressureRegulator`]: Bounded channel flow control.
//! - `ToolExecutor`, `LlmClient`, `Persistence`: Pluggable trait boundaries.
//!
//! ## Invariants upheld
//! - I-Shell-Runtime-OnlyIO: Core never performs I/O; shell handles all effects.
//! - I-Shell-Runtime-DeterministicClock: `SystemSignal::Tick` is the only time source.
//! - I-Shell-Backpressure-NoOverflow: `BackpressureRegulator` never exceeds capacity.
//! - I-Shell-ToolResult-PassThrough: Shell does not transform tool results by policy.
//!
//! Refs: docs/SPECS.md §Book III-A

pub mod effect_executor;
pub mod engine_watchdog;
pub mod http_client;
pub mod llm_client;
pub mod network_recovery;
pub mod shell;
pub mod signal_adapter;
pub mod telemetry;
pub mod transition_journal;
pub mod unified_event_bus;

// Re-export channel types from core so consumers need only one import.
pub use brioche_core::{
    AsyncTaskResult, BriocheEngine, BriocheEngineBuilder, BriochePlugin, ChatMessage, Effect,
    EngineInput, GovernanceNotification, Session, SystemSignal,
};
pub use effect_executor::{DefaultEffectExecutor, EffectExecutor, NoopPersistence, Persistence};
pub use engine_watchdog::{
    EngineWatchdog, EngineWatchdogHandle, RecoveryProcedure, WatchdogPing, WatchdogPong,
};
pub use http_client::{
    ALLOWED_SCHEMES, BLOCKED_HOSTS, DEFAULT_CONNECT_TIMEOUT, DEFAULT_MAX_REDIRECTS,
    DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_REQUEST_TIMEOUT, HttpClientError, build_http_client,
    read_body_with_size_limit, validate_url,
};
pub use llm_client::{LlmChunk, LlmClient, MockLlmClient};
pub use network_recovery::{ExponentialBackoff, NetworkRecovery, NoRetry};
pub use shell::{
    BackpressureRegulator, BriocheShell, DropPolicy, EchoToolExecutor, PersistenceMode,
    SessionCallback, ShellConfig, ShellError, StateSnapshot, TickEmitter, ToolExecutor,
};
pub use signal_adapter::{
    AsyncTaskResultAdapter, GovernanceNotificationAdapter, SignalMultiplexer, SystemSignalAdapter,
};
pub use telemetry::{
    Secret, TelemetryChannel, TelemetryEvent, TelemetryLevel, TelemetryPayload,
    install_default_subscriber,
};
pub use transition_journal::{JournalEntry, TransitionJournal};
pub use unified_event_bus::{EngineEnvelope, UnifiedEventBus};
