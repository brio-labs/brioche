//! `SignalMultiplexer` — canonical `SignalDrainOrder` implementation.
//!
//! Encapsulates the three separate channel adapters and guarantees
//! the invariant drainage order between transition cycles:
//! `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`.
//!
//! Refs: SPECS.md §Book III-A Ch 3, I-Shell-Drain-Atomic

use brioche_core::{SignalDrainBatch, SignalDrainOrder};
use tokio::sync::mpsc;

/// Standard `SignalDrainOrder` implementation.
///
/// The `SignalMultiplexer` owns the three channel receivers and drains
/// them atomically with respect to transition cycles. All signals
/// drained between two calls to `transition()` are treated as a single
/// batch, in canonical order.
///
/// This component is a shell extension; it modifies neither channels,
/// nor adapters, nor the kernel.
///
/// Refs: I-Shell-Drain-Atomic
pub struct SignalMultiplexer {
    system_rx: std::sync::Mutex<mpsc::Receiver<brioche_core::SystemSignal>>,
    governance_rx: std::sync::Mutex<mpsc::Receiver<brioche_core::GovernanceNotification>>,
    async_rx: std::sync::Mutex<mpsc::Receiver<brioche_core::AsyncTaskResult>>,
}

impl SignalMultiplexer {
    /// Create a new multiplexer from the three channel receivers.
    ///
    /// The receivers must be the ones paired with the adapters held
    /// by the async side of the shell.
    pub fn new(
        system_rx: mpsc::Receiver<brioche_core::SystemSignal>,
        governance_rx: mpsc::Receiver<brioche_core::GovernanceNotification>,
        async_rx: mpsc::Receiver<brioche_core::AsyncTaskResult>,
    ) -> Self {
        Self {
            system_rx: std::sync::Mutex::new(system_rx),
            governance_rx: std::sync::Mutex::new(governance_rx),
            async_rx: std::sync::Mutex::new(async_rx),
        }
    }
}

impl SignalDrainOrder for SignalMultiplexer {
    /// Drain all three channels in canonical order.
    ///
    /// # Invariants
    /// - `SystemSignal` is fully drained before `GovernanceNotification`.
    /// - `GovernanceNotification` is fully drained before `AsyncTaskResult`.
    /// - Within each channel, FIFO order is preserved.
    ///
    /// # Complexity
    /// O(n) where n = total pending events across all three channels.
    /// No heap allocation beyond the result vectors.
    fn drain(&self) -> SignalDrainBatch {
        let mut system_signals = Vec::new();
        let mut governance_notifications = Vec::new();
        let mut async_task_results = Vec::new();

        if let Ok(mut rx) = self.system_rx.lock() {
            while let Ok(signal) = rx.try_recv() {
                system_signals.push(signal);
            }
        }

        if let Ok(mut rx) = self.governance_rx.lock() {
            while let Ok(notification) = rx.try_recv() {
                governance_notifications.push(notification);
            }
        }

        if let Ok(mut rx) = self.async_rx.lock() {
            while let Ok(result) = rx.try_recv() {
                async_task_results.push(result);
            }
        }

        SignalDrainBatch {
            system_signals,
            governance_notifications,
            async_task_results,
        }
    }
}
