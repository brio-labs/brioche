//! Signal adapter, unified event bus, and signal-buffer injection contracts.

mod common;

use std::time::Duration;

use brioche_core::{EngineInput, SignalDrainOrder};
use brioche_shell_runtime::{
    AsyncTaskResultAdapter, GovernanceNotificationAdapter, SignalMultiplexer, SystemSignalAdapter,
    TickEmitter, UnifiedEventBus,
};

use common::{build_shell_with_recorder, is_idle, is_predicting, recorded_views};

#[tokio::test]
async fn signal_multiplexer_drains_canonical_order() {
    let (sys_adapter, sys_rx) = SystemSignalAdapter::new(16);
    let (gov_adapter, gov_rx) = GovernanceNotificationAdapter::new(16);
    let (async_adapter, async_rx) = AsyncTaskResultAdapter::new(16);

    // Send events in reverse canonical order.
    assert!(
        async_adapter
            .try_send(brioche_core::AsyncTaskResult::CpuTaskDone {
                task_id: "cpu1".into(),
                result: vec![1, 2, 3],
            })
            .is_ok()
    );
    assert!(
        gov_adapter
            .try_send(brioche_core::GovernanceNotification::PluginFaulted {
                plugin_name: "p1".into(),
                error: brioche_core::PluginError::Soft {
                    plugin_name: "p1".into(),
                    message: "oops".into(),
                },
            })
            .is_ok()
    );
    assert!(
        sys_adapter
            .try_send(brioche_core::SystemSignal::OperationCancelled)
            .is_ok()
    );

    let multiplexer = SignalMultiplexer::new(sys_rx, gov_rx, async_rx);
    let batch = multiplexer.drain();

    assert_eq!(batch.system_signals.len(), 1);
    assert!(
        matches!(
            batch.system_signals[0],
            brioche_core::SystemSignal::OperationCancelled
        ),
        "system signals should be drained first"
    );

    assert_eq!(batch.governance_notifications.len(), 1);
    assert!(
        matches!(
            batch.governance_notifications[0],
            brioche_core::GovernanceNotification::PluginFaulted { .. }
        ),
        "governance notifications should be drained second"
    );

    assert_eq!(batch.async_task_results.len(), 1);
    assert!(
        matches!(
            batch.async_task_results[0],
            brioche_core::AsyncTaskResult::CpuTaskDone { .. }
        ),
        "async task results should be drained third"
    );
}

#[tokio::test]
async fn unified_event_bus_fast_path_drains_directly() {
    let (sys_adapter, sys_rx) = SystemSignalAdapter::new(16);
    let (_gov_adapter, gov_rx) = GovernanceNotificationAdapter::new(16);
    let (_async_adapter, async_rx) = AsyncTaskResultAdapter::new(16);

    assert!(
        sys_adapter
            .try_send(brioche_core::SystemSignal::Tick { elapsed_ms: 42 })
            .is_ok()
    );

    let (bus, _producer_tx) = UnifiedEventBus::new(sys_rx, gov_rx, async_rx);
    let batch = bus.drain();

    assert_eq!(batch.system_signals.len(), 1);
    assert!(
        matches!(
            batch.system_signals[0],
            brioche_core::SystemSignal::Tick { elapsed_ms: 42 }
        ),
        "fast path should drain directly from underlying receiver"
    );
}

#[tokio::test]
async fn unified_event_bus_slow_path_consumes_envelopes() {
    let (_sys_adapter, sys_rx) = SystemSignalAdapter::new(16);
    let (_gov_adapter, gov_rx) = GovernanceNotificationAdapter::new(16);
    let (_async_adapter, async_rx) = AsyncTaskResultAdapter::new(16);

    let (bus, producer_tx) = UnifiedEventBus::new(sys_rx, gov_rx, async_rx);

    // Send an envelope batch through the producer channel.
    let batch = vec![brioche_shell_runtime::EngineEnvelope::Signal(
        brioche_core::SystemSignal::NetworkUnavailable {
            reason: "test".into(),
        },
    )];
    assert!(producer_tx.send(batch).await.is_ok());

    let drained = bus.drain();
    assert_eq!(drained.system_signals.len(), 1);
    assert!(
        matches!(
            drained.system_signals[0],
            brioche_core::SystemSignal::NetworkUnavailable { .. }
        ),
        "slow path should consume unified channel envelopes"
    );
}

#[tokio::test]
async fn tick_emitter_produces_ticks() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let emitter = TickEmitter::new(tx, 50);

    tokio::spawn(emitter.run());

    let first = match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
        Ok(Some(ev)) => ev,
        Ok(None) => unreachable!("mpsc channel closed"),
        Err(_) => unreachable!("should receive first tick within timeout"),
    };

    assert!(
        matches!(first, brioche_core::SystemSignal::Tick { .. }),
        "tick emitter should produce SystemSignal::Tick"
    );
}

#[tokio::test]
async fn shell_injects_signal_buffer_before_transition() {
    let (shell, views) = build_shell_with_recorder();

    assert!(
        shell
            .send_system_signal(brioche_core::SystemSignal::Tick { elapsed_ms: 1234 })
            .await
            .is_ok()
    );
    assert!(
        shell
            .send_governance_notification(brioche_core::GovernanceNotification::PluginFaulted {
                plugin_name: "test".into(),
                error: brioche_core::PluginError::Soft {
                    plugin_name: "test".into(),
                    message: "soft".into(),
                },
            })
            .await
            .is_ok()
    );
    assert!(
        shell
            .send_async_task_result(brioche_core::AsyncTaskResult::CpuTaskDone {
                task_id: "t1".into(),
                result: vec![],
            })
            .await
            .is_ok()
    );

    assert!(
        shell
            .send_input(EngineInput::UserMessage("hello".into()))
            .await
            .is_ok()
    );

    tokio::time::sleep(Duration::from_millis(100)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    let first = &views[0];
    assert!(
        is_predicting(&first.state, 1),
        "buffered signals should not prevent the user message transition"
    );

    let last = &views[views.len() - 1];
    assert!(is_idle(&last.state), "final state should be Idle");
    assert_eq!(last.history.len(), 2);

    assert!(shell.ready().await.is_ok());
}
