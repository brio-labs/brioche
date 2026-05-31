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
//! Refs: SPECS.md §Book III-A

pub mod backpressure;
pub mod effect_executor;
pub mod engine_watchdog;
pub mod llm_client;
pub mod network_recovery;
pub mod persistence_mode;
pub mod shell;
pub mod signal_adapter;
pub mod signal_multiplexer;
pub mod telemetry;
pub mod tick_emitter;
pub mod tool_executor;
pub mod transition_journal;
pub mod unified_event_bus;

pub use backpressure::{BackpressureRegulator, DropPolicy};
pub use effect_executor::{DefaultEffectExecutor, EffectExecutor, NoopPersistence, Persistence};
pub use engine_watchdog::{
    EngineWatchdog, EngineWatchdogHandle, RecoveryProcedure, WatchdogPing, WatchdogPong,
};
pub use llm_client::{LlmChunk, LlmClient, MockLlmClient};
pub use network_recovery::{ExponentialBackoff, NetworkRecovery, NoRetry};
pub use persistence_mode::PersistenceMode;
pub use shell::{BriocheShell, SessionCallback, ShellConfig, ShellError, StateSnapshot};
pub use signal_adapter::{
    AsyncTaskResultAdapter, GovernanceNotificationAdapter, SystemSignalAdapter,
};
pub use signal_multiplexer::SignalMultiplexer;
pub use telemetry::{TelemetryChannel, TelemetryEvent, TelemetryLevel, install_default_subscriber};
pub use tick_emitter::TickEmitter;
pub use tool_executor::{EchoToolExecutor, ToolExecutor};
pub use transition_journal::{JournalEntry, TransitionJournal};
pub use unified_event_bus::{EngineEnvelope, UnifiedEventBus};

// Re-export channel types from core so consumers need only one import.
pub use brioche_core::{
    AsyncTaskResult, BriocheEngine, BriocheEngineBuilder, BriochePlugin, ChatMessage, Effect,
    EngineInput, GovernanceNotification, Session, SystemSignal,
};
