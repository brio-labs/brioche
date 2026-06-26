//! Telemetry channel and non-blocking subscriber.
//!
//! The kernel remains telemetry-agnostic. Telemetry events transit
//! through a separate `TelemetryChannel` that the shell consumes via
//! its non-blocking subscriber.
//!
//! Refs: docs/SPECS.md §Book III-A Ch 1, I-Shell-Telemetry-NoKernel

use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

/// Severity level for a telemetry event.
/// Refs: docs/SPECS.md §Book III-A
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
/// A value that must never appear in telemetry output.
///
/// `Secret` wraps any serializable value and renders it as `"[REDACTED]"` in
/// both `Debug` and JSON. Use it for API keys, tokens, and other credentials
/// that transit through telemetry payloads.
///
/// The wrapped value can still be accessed explicitly via [`Secret::expose`]
/// for legitimate runtime use.
///
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, PartialEq, Eq)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Wrap a secret value.
    ///
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Access the underlying secret.
    ///
    /// Refs: docs/SPECS.md §Book III-A
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> std::fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl<T: Serialize> Serialize for Secret<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let _ = &self.0;
        serializer.serialize_str("[REDACTED]")
    }
}

/// Payload attached to a telemetry event.
///
/// Either a plain, loggable value or a redacted secret. Keeping the wrapper
/// at the payload boundary ensures secrets are never serialized accidentally.
///
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum TelemetryPayload {
    /// A plain, loggable value.
    Plain(Value),
    /// A redacted secret value.
    Secret(Secret<Value>),
}

impl TelemetryPayload {
    /// Create a plain payload.
    ///
    /// Refs: docs/SPECS.md §Book III-A
    pub fn plain(value: Value) -> Self {
        Self::Plain(value)
    }

    /// Create a redacted payload from a secret value.
    ///
    /// Refs: docs/SPECS.md §Book III-A
    pub fn secret(value: Value) -> Self {
        Self::Secret(Secret::new(value))
    }

    /// Return the underlying value if this payload is a secret.
    ///
    /// Refs: docs/SPECS.md §Book III-A
    pub fn expose_secret(&self) -> Option<&Value> {
        match self {
            Self::Secret(secret) => Some(secret.expose()),
            Self::Plain(_) => None,
        }
    }
}

/// A single telemetry event emitted by the shell.
///
/// Telemetry events are fire-and-forget. The kernel never blocks on
/// telemetry emission.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct TelemetryEvent {
    /// Optional structured payload. Use [`TelemetryPayload::secret`] for any
    /// value that contains credentials or other sensitive material.
    pub payload: Option<TelemetryPayload>,
    /// Severity level.
    pub level: TelemetryLevel,
    /// Logical source component (e.g., "watchdog", "effect_executor").
    pub source: String,
    /// Human-readable message.
    pub message: String,
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
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Emit a telemetry event.
    ///
    /// This is non-blocking. If the channel is full or there are no
    /// subscribers, the event is silently dropped.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn emit(
        &self,
        level: TelemetryLevel,
        source: impl Into<String>,
        message: impl Into<String>,
        payload: Option<TelemetryPayload>,
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
    /// Refs: docs/SPECS.md §Book III-A
    pub fn subscribe(&self) -> broadcast::Receiver<TelemetryEvent> {
        self.tx.subscribe()
    }
}

/// Default non-blocking telemetry subscriber.
///
/// Spawns a Tokio task that logs all telemetry events via `tracing`.
/// This is the default subscriber installed by `BriocheShell`.
/// Refs: docs/SPECS.md §Book III-A
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
