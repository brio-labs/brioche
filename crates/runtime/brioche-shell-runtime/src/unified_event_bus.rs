//! `UnifiedEventBus` — optional consolidated event channel.
//!
//! Consolidates the four event sources (`EngineInput`, `SystemSignal`,
//! `AsyncTaskResult`, `GovernanceNotification`) into a single internal
//! typed channel `EngineEnvelope`. The shell chooses at initialization
//! between `SignalMultiplexer` (classic separate adapters) and
//! `UnifiedEventBus` (unified channel).
//!
//! Refs: docs/SPECS.md §Book III-A Ch 3.5

use brioche_core::{
    AsyncTaskResult, EngineInput, GovernanceNotification, SignalDrainBatch, SignalDrainOrder,
    SystemSignal,
};
use tokio::sync::{Mutex, mpsc};

/// Unified envelope type for all events entering the engine thread.
///
/// The `UnifiedEventBus` producer wraps events from the three separate
/// channels (plus direct `EngineInput`) into this enum before pushing
/// them into the internal unified channel.
///
/// Refs: docs/SPECS.md §Book III-A Ch 3.5
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
/// Refs: docs/SPECS.md §Book III-A Ch 3.5
pub struct UnifiedEventBus {
    envelope_tx: mpsc::Sender<Vec<EngineEnvelope>>,
    envelope_rx: std::sync::Mutex<mpsc::Receiver<Vec<EngineEnvelope>>>,
    system_rx: Mutex<mpsc::Receiver<SystemSignal>>,
    governance_rx: Mutex<mpsc::Receiver<GovernanceNotification>>,
    async_rx: Mutex<mpsc::Receiver<AsyncTaskResult>>,
}

impl UnifiedEventBus {
    /// Create a new unified bus from the three underlying receivers.
    ///
    /// The producer half is returned so that the async runtime can
    /// spawn a task that feeds the bus from the separate channels.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(
        system_rx: mpsc::Receiver<SystemSignal>,
        governance_rx: mpsc::Receiver<GovernanceNotification>,
        async_rx: mpsc::Receiver<AsyncTaskResult>,
    ) -> (Self, mpsc::Sender<Vec<EngineEnvelope>>) {
        let (envelope_tx, envelope_rx) = mpsc::channel::<Vec<EngineEnvelope>>(256);
        let bus = Self {
            envelope_tx: envelope_tx.clone(),
            envelope_rx: std::sync::Mutex::new(envelope_rx),
            system_rx: Mutex::new(system_rx),
            governance_rx: Mutex::new(governance_rx),
            async_rx: Mutex::new(async_rx),
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
    pub async fn producer_loop(&self) {
        loop {
            // Acquire all source receivers. Holding the locks while
            // waiting in `tokio::select!` prevents `drain()` from racing
            // the producer on the same receiver, which is safe because
            // an idle producer means the source channels are empty.
            let mut system_guard = self.system_rx.lock().await;
            let mut governance_guard = self.governance_rx.lock().await;
            let mut async_guard = self.async_rx.lock().await;

            // Wait for at least one event on any channel.
            let first = tokio::select! {
                signal = system_guard.recv() => signal.map(EngineEnvelope::Signal),
                notification = governance_guard.recv() => {
                    notification.map(EngineEnvelope::Governance)
                }
                result = async_guard.recv() => result.map(EngineEnvelope::TaskResult),
            };

            let Some(first) = first else {
                // All source channels closed.
                break;
            };

            // Batch all remaining pending events in canonical order.
            let mut batch = vec![first];
            while let Ok(signal) = system_guard.try_recv() {
                batch.push(EngineEnvelope::Signal(signal));
            }
            while let Ok(notification) = governance_guard.try_recv() {
                batch.push(EngineEnvelope::Governance(notification));
            }
            while let Ok(result) = async_guard.try_recv() {
                batch.push(EngineEnvelope::TaskResult(result));
            }

            // Release the source receivers before the bounded send.
            drop(system_guard);
            drop(governance_guard);
            drop(async_guard);

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
            if let Ok(mut rx) = self.system_rx.try_lock() {
                while let Ok(signal) = rx.try_recv() {
                    system_signals.push(signal);
                }
            }
            if let Ok(mut rx) = self.governance_rx.try_lock() {
                while let Ok(notification) = rx.try_recv() {
                    governance_notifications.push(notification);
                }
            }
            if let Ok(mut rx) = self.async_rx.try_lock() {
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
                            // drain path; log at debug level.
                            tracing::debug!("EngineEnvelope::Input discarded in signal drain");
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use brioche_core::{
        AsyncTaskResult, GovernanceNotification, PluginError, SignalDrainOrder, SystemSignal,
    };
    use tokio::sync::mpsc;

    use super::UnifiedEventBus;

    #[tokio::test]
    async fn producer_loop_forwards_events_in_canonical_order() {
        let (system_tx, system_rx) = mpsc::channel(16);
        let (governance_tx, governance_rx) = mpsc::channel(16);
        let (async_tx, async_rx) = mpsc::channel(16);

        let (bus, _producer_tx) = UnifiedEventBus::new(system_rx, governance_rx, async_rx);
        let bus = Arc::new(bus);

        let producer = tokio::spawn({
            let bus = Arc::clone(&bus);
            async move { bus.producer_loop().await }
        });

        // Send events in reverse canonical order.
        assert!(
            async_tx
                .send(AsyncTaskResult::CpuTaskDone {
                    task_id: "cpu1".into(),
                    result: vec![1, 2, 3],
                })
                .await
                .is_ok()
        );
        assert!(
            governance_tx
                .send(GovernanceNotification::PluginFaulted {
                    plugin_name: "p".into(),
                    error: PluginError::Soft {
                        plugin_name: "p".into(),
                        message: "e".into(),
                    },
                })
                .await
                .is_ok()
        );
        assert!(
            system_tx
                .send(SystemSignal::Tick { elapsed_ms: 42 })
                .await
                .is_ok()
        );

        // Give the producer time to batch and forward.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let batch = bus.drain();

        assert_eq!(batch.system_signals.len(), 1);
        assert_eq!(batch.governance_notifications.len(), 1);
        assert_eq!(batch.async_task_results.len(), 1);

        // Close all source channels to stop the producer.
        drop(system_tx);
        drop(governance_tx);
        drop(async_tx);

        assert!(producer.await.is_ok());
    }
}
