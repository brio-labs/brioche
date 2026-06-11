//! Periodic `SystemSignal::Tick` emitter.
//!
//! Emits `SystemSignal::Tick` at a configurable interval for
//! consumption by `SubRoutineTimeoutPolicy` and other time-aware
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, interval};

/// Emits periodic ticks into a `SystemSignal` channel.
///
/// # Example
///
/// ```no_run
/// # async fn example() {
/// use brioche_core::SystemSignal;
/// use brioche_shell_runtime::TickEmitter;
/// use tokio::sync::mpsc;
///
/// let (tx, _rx) = mpsc::channel(64);
/// let emitter = TickEmitter::new(tx, 1000);
/// emitter.run().await;
/// # }
/// ```
/// Refs: SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct TickEmitter {
    tx: mpsc::Sender<brioche_core::SystemSignal>,
    interval_ms: u64,
    start: Instant,
}

impl TickEmitter {
    /// Create a tick emitter from a sender.
    ///
    /// `tx` — sender wired to the `SystemSignal` channel consumed by the shell.
    /// `interval_ms` — tick period in milliseconds (default: 1000).
    /// Refs: SPECS.md §Book III-A
    pub fn new(tx: mpsc::Sender<brioche_core::SystemSignal>, interval_ms: u64) -> Self {
        Self {
            tx,
            interval_ms,
            start: Instant::now(),
        }
    }

    /// Run the emitter loop until the receiver is dropped.
    ///
    /// This future never completes unless the channel closes.
    pub async fn run(self) {
        let mut ticker = interval(Duration::from_millis(self.interval_ms));
        let start = self.start;

        loop {
            ticker.tick().await;
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let signal = brioche_core::SystemSignal::Tick { elapsed_ms };
            if self.tx.send(signal).await.is_err() {
                break;
            }
        }
    }
}
