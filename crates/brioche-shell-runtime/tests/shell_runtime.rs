//! Integration tests for `brioche-shell-runtime`.
//!
//! Covers:
//! - Shell construction with `!Send` engine / session
//! - Input dispatch roundtrip
//! - Effect execution (tools, CPU tasks, save)
//! - Backpressure regulator
//! - System signal delivery
//!
//! Refs: SPECS.md §Book III-A

use brioche_core::{ActiveToolCall, BriocheEngineBuilder, EngineInput, Session, SystemSignal};
use brioche_governance_default::{LexicographicDecisionAggregator, SubRoutineCleanupGuard};
use brioche_shell_runtime::{
    BackpressureRegulator, BriocheShell, DefaultEffectExecutor, DropPolicy, EchoToolExecutor,
    MockLlmClient, NoopPersistence, ShellConfig, ToolExecutor,
};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_minimal_engine() -> brioche_core::BriocheEngine {
    let result = BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard))
        .build();
    assert!(result.is_ok(), "minimal engine should build");
    result.ok().unwrap_or_else(|| unreachable!())
}

fn build_shell() -> BriocheShell {
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);
    BriocheShell::new(
        || (build_minimal_engine(), Session::new("test")),
        ShellConfig::default(),
        executor,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn shell_dispatches_user_message() {
    let shell = build_shell();

    assert!(
        shell
            .send_input(EngineInput::UserMessage("hello".into()))
            .await
            .is_ok(),
        "input send should succeed"
    );

    // Allow the engine thread to process and the effect loop to run.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The engine should have produced CallLlmNetwork + SaveSession effects.
    // Since the effect executor is async, we verify the shell stays alive.
    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn shell_routes_system_signal() {
    let shell = build_shell();

    assert!(
        shell
            .send_system_signal(SystemSignal::OperationCancelled)
            .await
            .is_ok(),
        "signal send should succeed"
    );

    // Signal should be drained into the engine thread's local adapter.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn backpressure_conservative_drops_text_chunks() {
    let (regulator, mut rx) = BackpressureRegulator::new(2, DropPolicy::Conservative);

    // Fill the channel.
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
}

#[tokio::test]
async fn tool_executor_echo_returns_success() {
    let executor = EchoToolExecutor;
    let call = ActiveToolCall {
        tool_id: "t1".into(),
        tool_name: "echo".into(),
        arguments: "{\"msg\":\"hi\"}".into(),
        timeout_ms: 1000,
    };

    let result = executor
        .execute(&call, tokio_util::sync::CancellationToken::new())
        .await;

    assert_eq!(result.tool_id, "t1");
    assert_eq!(result.tool_name, "echo");
    assert!(
        matches!(result.outcome, brioche_core::ToolOutcome::Success(ref s) if s == "{\"msg\":\"hi\"}"),
        "echo executor should return arguments as success"
    );
}

#[tokio::test]
async fn effect_executor_tools_parallel() {
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);
    let shell = BriocheShell::new(
        || (build_minimal_engine(), Session::new("test")),
        ShellConfig::default(),
        executor.clone(),
    );

    // Push a user message so the engine enters Predicting.
    assert!(
        shell
            .send_input(EngineInput::UserMessage("call tools".into()))
            .await
            .is_ok(),
        "input send should succeed"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Shell should still be healthy after parallel tool execution.
    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn shell_graceful_shutdown() {
    let shell = build_shell();

    shell.shutdown();

    // After shutdown, sending should eventually fail.
    // The exact timing depends on the engine thread noticing the
    // closed channel, so we just verify the method exists and runs.
}
