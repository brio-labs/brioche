//! Channel adapters for separate event channels.
//!
//! Adapters accumulate `SystemSignal`, `AsyncTaskResult`, and
//! `GovernanceNotification` events in local queues. Between each
//! `transition()` cycle, the shell drains these queues and injects
/// them into the engine flow.
///
/// Refs: SPECS.md §1.4, I-Shell-Drain-Atomic
use brioche_core::{AsyncTaskResult, GovernanceNotification, SystemSignal};
use tokio::sync::mpsc;

/// Adapter for `SystemSignal` events.
///
/// Signals are produced by the shell (network failure, cancellation,
/// periodic tick) and consumed by governance plugins.
#[derive(Clone, Debug)]
pub struct SystemSignalAdapter {
    tx: mpsc::Sender<SystemSignal>,
}

impl SystemSignalAdapter {
    /// Create a new adapter with the given buffer capacity.
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<SystemSignal>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    /// Send a signal into the adapter.
    pub async fn send(
        &self,
        signal: SystemSignal,
    ) -> Result<(), mpsc::error::SendError<SystemSignal>> {
        self.tx.send(signal).await
    }

    /// Non-blocking try-send.
    pub fn try_send(
        &self,
        signal: SystemSignal,
    ) -> Result<(), mpsc::error::TrySendError<SystemSignal>> {
        self.tx.try_send(signal)
    }
}

/// Adapter for `AsyncTaskResult` events.
///
/// Results are produced by background tasks (CPU offload,
/// summarization, status checks) and consumed by plugins.
#[derive(Clone, Debug)]
pub struct AsyncTaskResultAdapter {
    tx: mpsc::Sender<AsyncTaskResult>,
}

impl AsyncTaskResultAdapter {
    /// Create a new adapter with the given buffer capacity.
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<AsyncTaskResult>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    /// Send a result into the adapter.
    pub async fn send(
        &self,
        result: AsyncTaskResult,
    ) -> Result<(), mpsc::error::SendError<AsyncTaskResult>> {
        self.tx.send(result).await
    }

    /// Non-blocking try-send.
    pub fn try_send(
        &self,
        result: AsyncTaskResult,
    ) -> Result<(), mpsc::error::TrySendError<AsyncTaskResult>> {
        self.tx.try_send(result)
    }
}

/// Adapter for `GovernanceNotification` events.
///
/// Notifications are produced by the shell when it detects a plugin
/// fault, and consumed by `QuarantineManager`.
#[derive(Clone, Debug)]
pub struct GovernanceNotificationAdapter {
    tx: mpsc::Sender<GovernanceNotification>,
}

impl GovernanceNotificationAdapter {
    /// Create a new adapter with the given buffer capacity.
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<GovernanceNotification>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    /// Send a notification into the adapter.
    pub async fn send(
        &self,
        notification: GovernanceNotification,
    ) -> Result<(), mpsc::error::SendError<GovernanceNotification>> {
        self.tx.send(notification).await
    }

    /// Non-blocking try-send.
    pub fn try_send(
        &self,
        notification: GovernanceNotification,
    ) -> Result<(), mpsc::error::TrySendError<GovernanceNotification>> {
        self.tx.try_send(notification)
    }
}
