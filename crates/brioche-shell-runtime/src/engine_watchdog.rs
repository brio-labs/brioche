//! `EngineWatchdog` — bi-directional heartbeat monitor.
//!
//! Monitors the engine thread's reactivity via a periodic ping-pong.
//! If the engine fails to respond within `max_response_delay_ms`, the
//! watchdog invokes a recovery procedure.
//!
//! The watchdog **never** attempts to forcibly kill the engine thread.
//!
//! Refs: SPECS.md §Book III-A Ch 4, I-Shell-Watchdog-NoKill

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

/// Ping message sent by the watchdog to the engine thread.
#[derive(Clone, Copy, Debug)]
pub struct WatchdogPing;

/// Pong message sent by the engine thread back to the watchdog.
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
/// Refs: SPECS.md §Book III-A Ch 4
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
#[derive(Clone, Debug)]
pub struct EngineWatchdog {
    heartbeat_interval_ms: u64,
    max_response_delay_ms: u64,
    recovery_procedure: RecoveryProcedure,
}

impl Default for EngineWatchdog {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 1000,
            max_response_delay_ms: 5000,
            recovery_procedure: RecoveryProcedure::NotifyAndDegrade,
        }
    }
}

impl EngineWatchdog {
    /// Create a watchdog with the given configuration.
    pub fn new(
        heartbeat_interval_ms: u64,
        max_response_delay_ms: u64,
        recovery_procedure: RecoveryProcedure,
    ) -> Self {
        Self {
            heartbeat_interval_ms,
            max_response_delay_ms,
            recovery_procedure,
        }
    }

    /// Run the watchdog loop.
    ///
    /// `ping_tx` — channel to send pings to the engine thread.
    /// `pong_rx` — channel to receive pongs from the engine thread.
    ///
    /// This future runs until the pong channel is closed.
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
                tracing::error!(
                    "watchdog recovery: SerializeAndRestart triggered. \
                     Sprint 10 placeholder — full restart logic deferred to Sprint 11."
                );
                // TODO(Sprint 11): serialize session DTO, flush Redb, restart engine.
            }
            RecoveryProcedure::NotifyAndDegrade => {
                tracing::error!(
                    "watchdog recovery: NotifyAndDegrade triggered. \
                     Sprint 10 placeholder — UI notification deferred to Shell Projection."
                );
                // TODO(Sprint 14): emit ForwardToUi effect for degraded mode banner.
            }
        }
    }
}

/// Handle held by the engine thread to respond to watchdog pings.
///
/// The engine thread loop should check `respond_if_pinged()` between
/// transition cycles.
pub struct EngineWatchdogHandle {
    ping_rx: mpsc::Receiver<WatchdogPing>,
    pong_tx: mpsc::Sender<WatchdogPong>,
    pending_inputs_counter: std::sync::Arc<AtomicU64>,
}

impl EngineWatchdogHandle {
    /// Create a new handle and the associated watchdog channels.
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
