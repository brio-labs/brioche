//! Telemetry channel and non-blocking subscriber.
//!
//! The kernel remains telemetry-agnostic. Telemetry events transit
//! through a separate `TelemetryChannel` that the shell consumes via
//! its non-blocking subscriber.
//!
//! Refs: SPECS.md §Book III-A Ch 1, I-Shell-Telemetry-NoKernel

use serde_json::Value;
use tokio::sync::broadcast;

/// Severity level for a telemetry event.
/// Refs: SPECS.md §Book III-A
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetryLevel {
    /// Detailed diagnostic information.
    Debug,
    /// General operational information.
    Info,
    /// Warning condition that may require attention.
    Warn,
    /// Error condition that prevented normal operation.
    Error,
}

/// A single telemetry event emitted by the shell.
///
/// Telemetry events are fire-and-forget. The kernel never blocks on
/// telemetry emission.
/// Refs: SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct TelemetryEvent {
    /// Severity level of the event.
    pub level: TelemetryLevel,
    /// Logical source component (e.g., "watchdog", "effect_executor").
    pub source: String,
    /// Human-readable message.
    pub message: String,
    /// Optional structured payload.
    pub payload: Option<Value>,
}

/// Telemetry channel — fire-and-forget broadcast.
///
/// The shell installs a non-blocking subscriber that logs or exports
/// telemetry events. Producers (effect handlers, watchdog, etc.) send
/// via `TelemetryChannel::emit()` without awaiting.
///
/// Refs: I-Shell-Telemetry-NoKernel
#[derive(Clone, Debug)]
pub struct TelemetryChannel {
    tx: broadcast::Sender<TelemetryEvent>,
}

impl TelemetryChannel {
    /// Create a new telemetry channel with the given buffer capacity.
    ///
    /// Events are dropped if no subscriber is listening and the buffer
    /// is full. This guarantees that telemetry never blocks the hot path.
    /// Refs: SPECS.md §Book III-A
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Emit a telemetry event.
    ///
    /// This is non-blocking. If the channel is full or there are no
    /// subscribers, the event is silently dropped.
    /// Refs: SPECS.md §Book III-A
    pub fn emit(
        &self,
        level: TelemetryLevel,
        source: impl Into<String>,
        message: impl Into<String>,
        payload: Option<Value>,
    ) {
        let event = TelemetryEvent {
            level,
            source: source.into(),
            message: message.into(),
            payload,
        };
        // broadcast::send is non-blocking; excess events are dropped.
        let _ = self.tx.send(event);
    }

    /// Subscribe to telemetry events.
    ///
    /// Returns a receiver that yields events emitted after the subscription.
    /// Refs: SPECS.md §Book III-A
    pub fn subscribe(&self) -> broadcast::Receiver<TelemetryEvent> {
        self.tx.subscribe()
    }
}

/// Default non-blocking telemetry subscriber.
///
/// Spawns a Tokio task that logs all telemetry events via `tracing`.
/// This is the default subscriber installed by `BriocheShell`.
/// Refs: SPECS.md §Book III-A
pub fn install_default_subscriber(channel: TelemetryChannel) {
    let mut rx = channel.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event.level {
                TelemetryLevel::Debug => {
                    tracing::debug!(source = %event.source, payload = ?event.payload, "{}", event.message);
                }
                TelemetryLevel::Info => {
                    tracing::info!(source = %event.source, payload = ?event.payload, "{}", event.message);
                }
                TelemetryLevel::Warn => {
                    tracing::warn!(source = %event.source, payload = ?event.payload, "{}", event.message);
                }
                TelemetryLevel::Error => {
                    tracing::error!(source = %event.source, payload = ?event.payload, "{}", event.message);
                }
            }
        }
    });
}
