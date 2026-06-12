//! `UnifiedEventBus` — optional consolidated event channel.
//!
//! Consolidates the four event sources (`EngineInput`, `SystemSignal`,
//! `AsyncTaskResult`, `GovernanceNotification`) into a single internal
//! typed channel `EngineEnvelope`. The shell chooses at initialization
//! between `SignalMultiplexer` (classic separate adapters) and
//! `UnifiedEventBus` (unified channel).
//!
//! Refs: SPECS.md §Book III-A Ch 3.5

use brioche_core::{
    AsyncTaskResult, EngineInput, GovernanceNotification, SignalDrainBatch, SignalDrainOrder,
    SystemSignal,
};
use tokio::sync::mpsc;

/// Unified envelope type for all events entering the engine thread.
///
/// The `UnifiedEventBus` producer wraps events from the three separate
/// channels (plus direct `EngineInput`) into this enum before pushing
/// them into the internal unified channel.
///
/// Refs: SPECS.md §Book III-A Ch 3.5
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineEnvelope {
    /// Direct engine input (user message, LLM stream, tool results).
    Input(EngineInput),
    /// System signal (tick, network failure, cancellation).
    Signal(SystemSignal),
    /// Result of an async CPU task.
    TaskResult(AsyncTaskResult),
    /// Governance notification (plugin faulted, epoch rejected).
    Governance(GovernanceNotification),
}

/// Consolidated event bus that feeds `SignalDrainOrder` from a single
/// internal channel.
///
/// The `UnifiedEventBus` implements a fast path: when the internal
/// channel is empty, it drains directly from the underlying adapters
/// without the indirection of the intermediate channel.
///
/// Refs: SPECS.md §Book III-A Ch 3.5
pub struct UnifiedEventBus {
    envelope_tx: mpsc::Sender<Vec<EngineEnvelope>>,
    envelope_rx: std::sync::Mutex<mpsc::Receiver<Vec<EngineEnvelope>>>,
    system_rx: std::sync::Mutex<mpsc::Receiver<SystemSignal>>,
    governance_rx: std::sync::Mutex<mpsc::Receiver<GovernanceNotification>>,
    async_rx: std::sync::Mutex<mpsc::Receiver<AsyncTaskResult>>,
}

impl UnifiedEventBus {
    /// Create a new unified bus from the three underlying receivers.
    ///
    /// The producer half is returned so that the async runtime can
    /// spawn a task that feeds the bus from the separate channels.
    /// Refs: SPECS.md §Book III-A
    pub fn new(
        system_rx: mpsc::Receiver<SystemSignal>,
        governance_rx: mpsc::Receiver<GovernanceNotification>,
        async_rx: mpsc::Receiver<AsyncTaskResult>,
    ) -> (Self, mpsc::Sender<Vec<EngineEnvelope>>) {
        let (envelope_tx, envelope_rx) = mpsc::channel::<Vec<EngineEnvelope>>(256);
        let bus = Self {
            envelope_tx: envelope_tx.clone(),
            envelope_rx: std::sync::Mutex::new(envelope_rx),
            system_rx: std::sync::Mutex::new(system_rx),
            governance_rx: std::sync::Mutex::new(governance_rx),
            async_rx: std::sync::Mutex::new(async_rx),
        };
        (bus, envelope_tx)
    }

    /// Producer loop that feeds the bus from the separate channels.
    ///
    /// This future should be spawned on the Tokio runtime. It runs
    /// until all source channels are closed.
    ///
    /// # Invariants
    /// - Canonical order is preserved within each batch:
    ///   `SystemSignal` > `GovernanceNotification` > `AsyncTaskResult`.
    ///
    /// # Cancel safety
    /// This loop holds only a local `Vec` across await points. Dropping
    /// it discards the in-progress batch; source channels remain intact.
    pub async fn producer_loop(
        &self,
        mut system_rx: mpsc::Receiver<SystemSignal>,
        mut governance_rx: mpsc::Receiver<GovernanceNotification>,
        mut async_rx: mpsc::Receiver<AsyncTaskResult>,
    ) {
        loop {
            let mut batch = Vec::new();

            // Drain system signals.
            while let Ok(signal) = system_rx.try_recv() {
                batch.push(EngineEnvelope::Signal(signal));
            }

            // Drain governance notifications.
            while let Ok(notification) = governance_rx.try_recv() {
                batch.push(EngineEnvelope::Governance(notification));
            }

            // Drain async task results.
            while let Ok(result) = async_rx.try_recv() {
                batch.push(EngineEnvelope::TaskResult(result));
            }

            if batch.is_empty() {
                // No events this cycle — yield to avoid busy-waiting.
                tokio::task::yield_now().await;
                continue;
            }

            if self.envelope_tx.send(batch).await.is_err() {
                break;
            }
        }
    }
}

impl SignalDrainOrder for UnifiedEventBus {
    /// Drain with fast-path bypass.
    ///
    /// If the internal unified channel has pending batches, they are
    /// consumed and flattened. Otherwise, the bus drains directly from
    /// the underlying adapters, avoiding intermediate channel overhead.
    ///
    /// # Complexity
    /// O(n) where n = total pending events across all channels.
    /// Fast path avoids intermediate Vec allocations when the unified
    /// channel is empty.
    fn drain(&self) -> SignalDrainBatch {
        let mut system_signals = Vec::new();
        let mut governance_notifications = Vec::new();
        let mut async_task_results = Vec::new();

        // Fast path: drain directly from underlying receivers if the
        // unified channel is empty.
        let has_unified_pending = if let Ok(rx) = self.envelope_rx.lock() {
            !rx.is_empty()
        } else {
            false
        };

        if !has_unified_pending {
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
            return SignalDrainBatch {
                system_signals,
                governance_notifications,
                async_task_results,
            };
        }

        // Slow path: consume from the unified channel.
        if let Ok(mut rx) = self.envelope_rx.lock() {
            while let Ok(batch) = rx.try_recv() {
                for envelope in batch {
                    match envelope {
                        EngineEnvelope::Signal(s) => system_signals.push(s),
                        EngineEnvelope::Governance(g) => governance_notifications.push(g),
                        EngineEnvelope::TaskResult(r) => async_task_results.push(r),
                        EngineEnvelope::Input(_) => {
                            // EngineInput should not appear in the signal
                            // drain path; ignore or log.
                        }
                    }
                }
            }
        }

        SignalDrainBatch {
            system_signals,
            governance_notifications,
            async_task_results,
        }
    }
}
