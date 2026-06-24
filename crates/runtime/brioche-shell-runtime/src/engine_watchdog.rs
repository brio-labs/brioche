//! `EngineWatchdog` — bi-directional heartbeat monitor.
//!
//! Monitors the engine thread's reactivity via a periodic ping-pong.
//! If the engine fails to respond within `max_response_delay_ms`, the
//! watchdog invokes a recovery procedure.
//!
//! The watchdog **never** attempts to forcibly kill the engine thread.
//!
//! Refs: docs/SPECS.md §Book III-A Ch 4, I-Shell-Watchdog-NoKill

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use crate::telemetry::{TelemetryChannel, TelemetryLevel};

/// Ping message sent by the watchdog to the engine thread.
///
/// Refs: I-Shell-Watchdog-NoKill
#[derive(Clone, Copy, Debug)]
pub struct WatchdogPing;

/// Pong message sent by the engine thread back to the watchdog.
///
/// Refs: I-Shell-Watchdog-NoKill
#[derive(Clone, Copy, Debug)]
pub struct WatchdogPong {
    /// Last processed epoch at the time of the pong.
    pub last_epoch: u64,
    /// Number of inputs awaiting processing in the channel.
    pub pending_inputs: usize,
}

/// Recovery procedure invoked when the engine thread is unresponsive.
///
/// The shell configures this at startup. The default implementation
/// logs a critical error and emits a telemetry event.
///
/// Refs: docs/SPECS.md §Book III-A Ch 4
#[derive(Clone, Debug)]
pub enum RecoveryProcedure {
    /// Emergency serialization + engine restart with session restored
    /// from Redb + replay of transitions from `TransitionJournal` if
    /// available.
    ///
    /// Sprint 10 placeholder: logs and emits telemetry. Full restart
    /// logic deferred to Sprint 11 (`TransitionJournal`).
    SerializeAndRestart,
    /// UI notification + degraded mode (session lost, Redb history intact).
    ///
    /// Sprint 10 placeholder: logs and emits telemetry.
    NotifyAndDegrade,
}

/// Bi-directional heartbeat watchdog.
///
/// The watchdog runs on the Tokio runtime. It periodically sends pings
/// to the engine thread via a channel and waits for pongs. If the
/// response delay exceeds `max_response_delay_ms`, the configured
/// `RecoveryProcedure` is triggered.
///
/// Refs: I-Shell-Watchdog-NoKill, I-Shell-Watchdog-Recovery
#[derive(Clone)]
pub struct EngineWatchdog {
    heartbeat_interval_ms: u64,
    max_response_delay_ms: u64,
    recovery_procedure: RecoveryProcedure,
    /// Optional transition journal for recovery replay after restart.
    ///
    /// Refs: I-Shell-TransitionJournal
    transition_journal: Option<Arc<crate::TransitionJournal>>,
    /// Telemetry channel for recovery events.
    telemetry: TelemetryChannel,
    /// Optional handler invoked for `SerializeAndRestart` recovery.
    serialize_and_restart_handler: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Optional handler invoked for `NotifyAndDegrade` recovery.
    notify_and_degrade_handler: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl std::fmt::Debug for EngineWatchdog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineWatchdog")
            .field("heartbeat_interval_ms", &self.heartbeat_interval_ms)
            .field("max_response_delay_ms", &self.max_response_delay_ms)
            .field("recovery_procedure", &self.recovery_procedure)
            .field("transition_journal", &self.transition_journal)
            .field("telemetry", &self.telemetry)
            .field(
                "serialize_and_restart_handler",
                &self
                    .serialize_and_restart_handler
                    .as_ref()
                    .map(|_| "<handler>"),
            )
            .field(
                "notify_and_degrade_handler",
                &self
                    .notify_and_degrade_handler
                    .as_ref()
                    .map(|_| "<handler>"),
            )
            .finish()
    }
}

impl Default for EngineWatchdog {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 1000,
            max_response_delay_ms: 5000,
            recovery_procedure: RecoveryProcedure::NotifyAndDegrade,
            transition_journal: None,
            telemetry: TelemetryChannel::new(64),
            serialize_and_restart_handler: None,
            notify_and_degrade_handler: None,
        }
    }
}

impl EngineWatchdog {
    /// Attach a `TransitionJournal` for recovery replay.
    ///
    /// If the engine restarts, the watchdog will read unacknowledged
    /// entries from the journal and replay them.
    ///
    /// Refs: I-Shell-TransitionJournal
    pub fn with_transition_journal(mut self, journal: Arc<crate::TransitionJournal>) -> Self {
        self.transition_journal = Some(journal);
        self
    }

    /// Attach the telemetry channel used to emit recovery events.
    ///
    /// Refs: I-Shell-Telemetry-NoKernel
    pub fn with_telemetry(mut self, telemetry: TelemetryChannel) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Set the handler invoked when `SerializeAndRestart` recovery is triggered.
    ///
    /// Refs: I-Shell-Watchdog-Recovery
    pub fn with_serialize_and_restart_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.serialize_and_restart_handler = Some(Arc::new(handler));
        self
    }

    /// Set the handler invoked when `NotifyAndDegrade` recovery is triggered.
    ///
    /// Refs: I-Shell-Watchdog-Recovery
    pub fn with_notify_and_degrade_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.notify_and_degrade_handler = Some(Arc::new(handler));
        self
    }
}

impl EngineWatchdog {
    /// Create a watchdog with the given configuration.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(
        heartbeat_interval_ms: u64,
        max_response_delay_ms: u64,
        recovery_procedure: RecoveryProcedure,
    ) -> Self {
        Self {
            heartbeat_interval_ms,
            max_response_delay_ms,
            recovery_procedure,
            transition_journal: None,
            telemetry: TelemetryChannel::new(64),
            serialize_and_restart_handler: None,
            notify_and_degrade_handler: None,
        }
    }

    /// Run the watchdog loop.
    ///
    /// `ping_tx` — channel to send pings to the engine thread.
    /// `pong_rx` — channel to receive pongs from the engine thread.
    ///
    /// This future runs until the pong channel is closed.
    ///
    /// # Cancel safety
    /// This loop holds only local variables across await points. Dropping
    /// it stops heartbeat monitoring; the engine thread continues running.
    pub async fn run(
        self,
        ping_tx: mpsc::Sender<WatchdogPing>,
        mut pong_rx: mpsc::Receiver<WatchdogPong>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_millis(self.heartbeat_interval_ms));

        loop {
            interval.tick().await;

            let ping_time = Instant::now();
            if ping_tx.send(WatchdogPing).await.is_err() {
                tracing::warn!("watchdog: engine thread disconnected (ping channel closed)");
                break;
            }

            let timeout = Duration::from_millis(self.max_response_delay_ms);
            match tokio::time::timeout(timeout, pong_rx.recv()).await {
                Ok(Some(pong)) => {
                    let elapsed_ms = ping_time.elapsed().as_millis() as u64;
                    tracing::debug!(
                        last_epoch = pong.last_epoch,
                        pending_inputs = pong.pending_inputs,
                        elapsed_ms,
                        "watchdog: pong received"
                    );
                }
                Ok(None) => {
                    tracing::warn!("watchdog: pong channel closed by engine thread");
                    break;
                }
                Err(_) => {
                    tracing::error!(
                        max_response_delay_ms = self.max_response_delay_ms,
                        "watchdog: engine thread non-responsive — triggering recovery"
                    );
                    self.execute_recovery().await;
                }
            }
        }
    }

    async fn execute_recovery(&self) {
        match self.recovery_procedure {
            RecoveryProcedure::SerializeAndRestart => {
                let unacknowledged = self
                    .transition_journal
                    .as_ref()
                    .map_or(0, |journal| journal.read_unacknowledged().len());
                tracing::error!(
                    unacknowledged,
                    "watchdog recovery: SerializeAndRestart triggered"
                );
                self.telemetry.emit(
                    TelemetryLevel::Error,
                    "watchdog",
                    format!(
                        "serialize-and-restart recovery procedure triggered ({} unacknowledged journal entries)",
                        unacknowledged
                    ),
                    None,
                );
                if let Some(handler) = &self.serialize_and_restart_handler {
                    handler();
                }
            }
            RecoveryProcedure::NotifyAndDegrade => {
                tracing::error!("watchdog recovery: NotifyAndDegrade triggered");
                self.telemetry.emit(
                    TelemetryLevel::Error,
                    "watchdog",
                    "degraded mode recovery procedure triggered",
                    None,
                );
                if let Some(handler) = &self.notify_and_degrade_handler {
                    handler();
                }
            }
        }
    }
}

/// Handle held by the engine thread to respond to watchdog pings.
///
/// The engine thread loop should check `respond_if_pinged()` between
/// transition cycles.
/// Refs: docs/SPECS.md §Book III-A
pub struct EngineWatchdogHandle {
    ping_rx: mpsc::Receiver<WatchdogPing>,
    pong_tx: mpsc::Sender<WatchdogPong>,
    pending_inputs_counter: std::sync::Arc<AtomicU64>,
}

impl EngineWatchdogHandle {
    /// Create a new handle and the associated watchdog channels.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(
        pending_inputs_counter: std::sync::Arc<AtomicU64>,
    ) -> (
        Self,
        mpsc::Sender<WatchdogPing>,
        mpsc::Receiver<WatchdogPong>,
    ) {
        let (ping_tx, ping_rx) = mpsc::channel::<WatchdogPing>(4);
        let (pong_tx, pong_rx) = mpsc::channel::<WatchdogPong>(4);
        let handle = Self {
            ping_rx,
            pong_tx,
            pending_inputs_counter,
        };
        (handle, ping_tx, pong_rx)
    }

    /// Check for a pending ping and respond with a pong.
    ///
    /// `last_epoch` — the current epoch from `ExtensionStorage`.
    ///
    /// This method is non-blocking. It should be called by the engine
    /// thread loop between transition cycles.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn respond_if_pinged(&mut self, last_epoch: u64) {
        if let Ok(_ping) = self.ping_rx.try_recv() {
            let pending_inputs = self.pending_inputs_counter.load(Ordering::Relaxed) as usize;
            let pong = WatchdogPong {
                last_epoch,
                pending_inputs,
            };
            // Best-effort send; if the watchdog has dropped, we ignore.
            let _ = self.pong_tx.try_send(pong);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{EngineWatchdog, RecoveryProcedure};
    use crate::telemetry::{TelemetryChannel, TelemetryLevel};

    #[tokio::test]
    async fn serialize_and_restart_emits_error_telemetry()
    -> Result<(), tokio::sync::broadcast::error::RecvError> {
        let telemetry = TelemetryChannel::new(16);
        let mut rx = telemetry.subscribe();

        let watchdog = EngineWatchdog::new(1000, 5000, RecoveryProcedure::SerializeAndRestart)
            .with_telemetry(telemetry);

        watchdog.execute_recovery().await;

        let event = rx.recv().await?;

        assert_eq!(event.level, TelemetryLevel::Error);
        assert_eq!(event.source, "watchdog");
        assert!(
            event.message.contains("serialize-and-restart"),
            "message should indicate serialize-and-restart recovery: {}",
            event.message
        );
        Ok(())
    }

    #[tokio::test]
    async fn notify_and_degrade_invokes_handler() {
        let telemetry = TelemetryChannel::new(16);
        let invoked = Arc::new(AtomicBool::new(false));
        let invoked_for_handler = Arc::clone(&invoked);

        let watchdog = EngineWatchdog::new(1000, 5000, RecoveryProcedure::NotifyAndDegrade)
            .with_telemetry(telemetry)
            .with_notify_and_degrade_handler(move || {
                invoked_for_handler.store(true, Ordering::SeqCst);
            });

        watchdog.execute_recovery().await;

        assert!(
            invoked.load(Ordering::SeqCst),
            "handler should have been invoked"
        );
    }

    #[tokio::test]
    async fn default_recovery_does_not_panic_and_emits_telemetry() {
        let telemetry = TelemetryChannel::new(16);
        let mut rx = telemetry.subscribe();

        let watchdog = EngineWatchdog::new(1000, 5000, RecoveryProcedure::NotifyAndDegrade)
            .with_telemetry(telemetry);

        watchdog.execute_recovery().await;

        assert!(
            rx.recv().await.is_ok(),
            "a telemetry event should be emitted even with no handler"
        );
    }
}
