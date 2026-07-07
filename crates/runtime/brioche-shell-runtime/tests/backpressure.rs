//! Backpressure regulator contracts.

use std::time::Duration;

use brioche_core::EngineInput;
use brioche_shell_runtime::{BackpressureRegulator, DropPolicy};

#[tokio::test]
async fn backpressure_conservative_drops_text_chunks() {
    let (regulator, mut rx) = BackpressureRegulator::new(2, DropPolicy::Conservative);

    assert_eq!(regulator.capacity(), 2, "capacity should match constructor");

    assert!(
        regulator
            .send(EngineInput::UserMessage("a".into()))
            .await
            .is_ok()
    );
    assert!(
        regulator
            .send(EngineInput::UserMessage("b".into()))
            .await
            .is_ok()
    );

    // The channel must never exceed its configured capacity.
    assert!(
        rx.len() <= 2,
        "conservative mode must keep the channel within capacity"
    );

    // A text chunk under pressure should be dropped (returns Ok without blocking).
    let chunk = brioche_core::StreamEvent::TextChunk {
        path: Default::default(),
        chunk: bytes::Bytes::from("c"),
    };
    assert!(
        regulator.send(EngineInput::LlmStream(chunk)).await.is_ok(),
        "text chunk should be dropped under pressure without error"
    );

    // Drain the channel.
    let mut count = 0;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(10), rx.recv()).await {
        count += 1;
    }

    // Conservative mode drops the text chunk, so we expect exactly 2.
    assert_eq!(
        count, 2,
        "conservative backpressure should drop intermediate text chunks under pressure"
    );
}

#[tokio::test]
async fn backpressure_strict_blocks_until_capacity() {
    let (regulator, mut rx) = BackpressureRegulator::new(2, DropPolicy::Strict);

    assert_eq!(regulator.capacity(), 2, "capacity should match constructor");

    assert!(
        regulator
            .send(EngineInput::UserMessage("a".into()))
            .await
            .is_ok()
    );
    assert!(
        regulator
            .send(EngineInput::UserMessage("b".into()))
            .await
            .is_ok()
    );

    assert_eq!(
        rx.len(),
        2,
        "strict mode should fill the channel to capacity"
    );

    // In strict mode, the third send should block until we drain.
    let send_fut = regulator.send(EngineInput::UserMessage("c".into()));

    // Drain one slot.
    let drained = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .is_ok();
    assert!(drained, "should drain within timeout");

    // Now the send can complete.
    let completed = tokio::time::timeout(Duration::from_millis(100), send_fut)
        .await
        .is_ok();
    assert!(completed, "send should complete after capacity is freed");

    // Drain the remaining messages.
    let mut count = 0;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(10), rx.recv()).await {
        count += 1;
    }
    assert_eq!(count, 2, "strict mode should deliver all three messages");
}
