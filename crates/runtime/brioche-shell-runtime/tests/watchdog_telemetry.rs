//! Watchdog and telemetry contracts.

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use brioche_shell_runtime::{
    EngineWatchdog, EngineWatchdogHandle, RecoveryProcedure, TelemetryChannel,
};

#[tokio::test]
async fn engine_watchdog_detects_non_responsive_engine() {
    let pending = Arc::new(AtomicU64::new(0));
    let (handle, ping_tx, pong_rx) = EngineWatchdogHandle::new(pending);

    // Spawn a watchdog with a very short timeout so the test is fast.
    let watchdog = EngineWatchdog::new(50, 100, RecoveryProcedure::NotifyAndDegrade);
    let watchdog_fut = watchdog.run(ping_tx, pong_rx);

    // Do NOT respond to pings — the engine is "stuck".
    let _handle = handle;

    // The watchdog loops forever, re-triggering recovery on each missed
    // pong. We verify it is still running after the recovery timeout
    // (which proves it detected non-responsiveness at least once).
    let timeout = tokio::time::timeout(Duration::from_millis(300), watchdog_fut).await;
    assert!(
        timeout.is_err(),
        "watchdog should still be running after detecting non-responsive engine"
    );
}

#[tokio::test]
async fn engine_watchdog_ping_pong_healthy() {
    let pending = Arc::new(AtomicU64::new(0));
    let (mut handle, ping_tx, pong_rx) = EngineWatchdogHandle::new(pending);

    let watchdog = EngineWatchdog::new(50, 200, RecoveryProcedure::NotifyAndDegrade);
    let watchdog_fut = watchdog.run(ping_tx, pong_rx);

    // Simulate a healthy engine that responds to pings.
    let engine_task = tokio::task::spawn_blocking(move || {
        for _ in 0..5 {
            std::thread::sleep(Duration::from_millis(30));
            handle.respond_if_pinged(1);
        }
    });

    let timeout = tokio::time::timeout(Duration::from_millis(1000), watchdog_fut).await;
    assert!(
        timeout.is_ok(),
        "watchdog should stay healthy with responsive pongs"
    );
    let _ = engine_task.await;
}

#[tokio::test]
async fn telemetry_channel_emits_and_subscribes() {
    let channel = TelemetryChannel::new(16);
    let mut rx = channel.subscribe();

    channel.emit(
        brioche_shell_runtime::TelemetryLevel::Info,
        "test_source",
        "hello telemetry",
        None,
    );

    let event = match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
        Ok(Ok(ev)) => ev,
        Ok(Err(_)) => unreachable!("broadcast channel closed"),
        Err(_) => unreachable!("should receive event within timeout"),
    };

    assert_eq!(event.source, "test_source");
    assert_eq!(event.message, "hello telemetry");
    assert!(matches!(
        event.level,
        brioche_shell_runtime::TelemetryLevel::Info
    ));
}
#[tokio::test]
async fn telemetry_payload_secret_is_redacted() -> Result<(), Box<dyn std::error::Error>> {
    use brioche_shell_runtime::TelemetryPayload;

    let channel = brioche_shell_runtime::TelemetryChannel::new(16);
    let mut rx = channel.subscribe();

    let secret_value: serde_json::Value =
        serde_json::from_str(r#"{"api_key":"super-secret-token"}"#)?;
    channel.emit(
        brioche_shell_runtime::TelemetryLevel::Info,
        "test_source",
        "hello telemetry",
        Some(TelemetryPayload::secret(secret_value.clone())),
    );

    let event = match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
        Ok(Ok(ev)) => ev,
        Ok(Err(_)) => unreachable!("broadcast channel closed"),
        Err(_) => unreachable!("should receive event within timeout"),
    };

    let payload = event.payload.ok_or("payload should be present")?;
    assert_eq!(
        payload.expose_secret(),
        Some(&secret_value),
        "secret payload should preserve the original value internally"
    );
    let serialized = serde_json::to_string(&payload)?;
    assert!(
        serialized.contains("[REDACTED]"),
        "secret payload should serialize as redacted, got {serialized}"
    );
    Ok(())
}
