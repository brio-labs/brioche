//! Channel adapters for separate event channels.
//!
//! Adapters bridge async channels into the synchronous engine thread.
//! Between each `transition()` cycle, the shell drains the receivers
//! and injects pending events into `ExtensionStorage` as `SignalBuffer`.
///
/// Refs: docs/SPECS.md §1.4, I-Shell-Drain-Atomic
use brioche_core::{AsyncTaskResult, GovernanceNotification, SystemSignal};
use tokio::sync::mpsc;

/// Adapter for `SystemSignal` events.
///
/// Signals are produced by the shell (network failure, cancellation,
/// periodic tick) and consumed by governance plugins.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct SystemSignalAdapter {
    tx: mpsc::Sender<SystemSignal>,
}

impl SystemSignalAdapter {
    /// Create a new adapter with the given buffer capacity.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<SystemSignal>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    /// Send a signal into the adapter.
    ///
    /// # Cancel safety
    /// This future holds no locks across await points. Dropping it before
    /// completion only fails to enqueue the signal.
    pub async fn send(
        &self,
        signal: SystemSignal,
    ) -> Result<(), mpsc::error::SendError<SystemSignal>> {
        self.tx.send(signal).await
    }

    /// Non-blocking try-send.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn try_send(
        &self,
        signal: SystemSignal,
    ) -> Result<(), mpsc::error::TrySendError<SystemSignal>> {
        self.tx.try_send(signal)
    }

    /// Drain all pending signals from the receiver.
    ///
    /// Called by the engine thread loop between transition cycles.
    ///
    /// Refs: I-Shell-Drain-Atomic
    pub fn drain(receiver: &mut mpsc::Receiver<SystemSignal>) -> Vec<SystemSignal> {
        let mut drained = Vec::new();
        while let Ok(signal) = receiver.try_recv() {
            drained.push(signal);
        }
        drained
    }
}

/// Adapter for `AsyncTaskResult` events.
///
/// Results are produced by background tasks (CPU offload,
/// summarization, status checks) and consumed by plugins.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct AsyncTaskResultAdapter {
    tx: mpsc::Sender<AsyncTaskResult>,
}

impl AsyncTaskResultAdapter {
    /// Create a new adapter with the given buffer capacity.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<AsyncTaskResult>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    /// Send a result into the adapter.
    ///
    /// # Cancel safety
    /// This future holds no locks across await points. Dropping it before
    /// completion only fails to enqueue the result.
    pub async fn send(
        &self,
        result: AsyncTaskResult,
    ) -> Result<(), mpsc::error::SendError<AsyncTaskResult>> {
        self.tx.send(result).await
    }

    /// Non-blocking try-send.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn try_send(
        &self,
        result: AsyncTaskResult,
    ) -> Result<(), mpsc::error::TrySendError<AsyncTaskResult>> {
        self.tx.try_send(result)
    }

    /// Drain all pending results from the receiver.
    ///
    /// Called by the engine thread loop between transition cycles.
    ///
    /// Refs: I-Shell-Drain-Atomic
    pub fn drain(receiver: &mut mpsc::Receiver<AsyncTaskResult>) -> Vec<AsyncTaskResult> {
        let mut drained = Vec::new();
        while let Ok(result) = receiver.try_recv() {
            drained.push(result);
        }
        drained
    }
}

/// Adapter for `GovernanceNotification` events.
///
/// Notifications are produced by the shell when it detects a plugin
/// fault, and consumed by `QuarantineManager`.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct GovernanceNotificationAdapter {
    tx: mpsc::Sender<GovernanceNotification>,
}

impl GovernanceNotificationAdapter {
    /// Create a new adapter with the given buffer capacity.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<GovernanceNotification>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    /// Send a notification into the adapter.
    ///
    /// # Cancel safety
    /// This future holds no locks across await points. Dropping it before
    /// completion only fails to enqueue the notification.
    pub async fn send(
        &self,
        notification: GovernanceNotification,
    ) -> Result<(), mpsc::error::SendError<GovernanceNotification>> {
        self.tx.send(notification).await
    }

    /// Non-blocking try-send.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn try_send(
        &self,
        notification: GovernanceNotification,
    ) -> Result<(), mpsc::error::TrySendError<GovernanceNotification>> {
        self.tx.try_send(notification)
    }

    /// Drain all pending notifications from the receiver.
    ///
    /// Called by the engine thread loop between transition cycles.
    ///
    /// Refs: I-Shell-Drain-Atomic
    pub fn drain(
        receiver: &mut mpsc::Receiver<GovernanceNotification>,
    ) -> Vec<GovernanceNotification> {
        let mut drained = Vec::new();
        while let Ok(notification) = receiver.try_recv() {
            drained.push(notification);
        }
        drained
    }
}

// ---------------------------------------------------------------------------
// SignalMultiplexer (merged from signal_multiplexer.rs)
// ---------------------------------------------------------------------------

use brioche_core::{SignalDrainBatch, SignalDrainOrder};

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
    /// Refs: docs/SPECS.md §Book III-A
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
