//! Tests for `DefaultEffectExecutor` covering every `Effect` variant.
//!
//! Uses `MockLlmClient` and `EchoToolExecutor` to exercise the default
//! effect dispatcher without network I/O, and observes loopback
//! `EngineInput`s / `AsyncTaskResult`s through a test shell.
//!
//! Refs: I-Shell-Runtime-OnlyIO, I-Shell-EffectExecutor-Construction

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use brioche_core::{
    ActiveToolCall, AsyncTaskResult, ChatMessage, EngineInput, ErrorCode, ErrorDetail, StreamEvent,
    SubRoutineHandle, SystemSignal, ToolOutcome, ToolResultDTO, UiWidget,
};
use brioche_shell_runtime::effect_executor::CpuTaskRegistry;
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, EffectExecutor, MockLlmClient,
    NoopPersistence, Persistence, PersistenceMode, ShellError,
};
use tokio::sync::RwLock;

/// Persistence layer that counts invocations for test assertions.
#[derive(Clone, Debug, Default)]
struct CountingPersistence {
    save_session_count: Arc<AtomicU64>,
    save_blob_count: Arc<AtomicU64>,
    gc_count: Arc<AtomicU64>,
}

#[async_trait::async_trait]
impl Persistence for CountingPersistence {
    async fn save_session(&self, _session_id: &str) -> Result<(), ShellError> {
        self.save_session_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn save_plugin_blob(&self, _plugin_id: &str, _data: Vec<u8>) -> Result<(), ShellError> {
        self.save_blob_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn gc(&self, _session_id: &str) -> Result<u64, ShellError> {
        Ok(self.gc_count.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

fn echo_executor() -> DefaultEffectExecutor<EchoToolExecutor, MockLlmClient, NoopPersistence> {
    DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
}

#[tokio::test]
async fn noop_persistence_save_session_succeeds() -> Result<(), ShellError> {
    let persistence = NoopPersistence;
    persistence.save_session("test-session").await
}

#[tokio::test]
async fn noop_persistence_save_plugin_blob_succeeds() -> Result<(), ShellError> {
    let persistence = NoopPersistence;
    persistence
        .save_plugin_blob("test-plugin", vec![1, 2, 3, 4])
        .await
}

#[tokio::test]
async fn noop_persistence_is_default() -> Result<(), ShellError> {
    let persistence: NoopPersistence = Default::default();
    persistence.save_session("default-test").await
}

#[tokio::test]
async fn call_llm_network_streams_text_and_done() -> Result<(), ShellError> {
    let (shell, mut input_rx, _system_rx, _async_rx, _gov_rx) = BriocheShell::test_with_channels();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient {
            chunks: vec!["Hello".into(), " ".into(), "world".into()],
        },
        NoopPersistence,
    );

    executor.call_llm(&shell).await?;

    let mut chunks = Vec::new();
    while let Ok(input) = input_rx.try_recv() {
        match input {
            EngineInput::LlmStream(event) => chunks.push(event),
            other => {
                return Err(ShellError::EffectExecution(format!(
                    "unexpected engine input: {other:?}"
                )));
            }
        }
    }

    assert_eq!(chunks.len(), 4, "expected three text chunks plus Done");
    assert!(
        matches!(
            &chunks[0],
            StreamEvent::TextChunk { chunk, .. } if chunk == "Hello"
        ),
        "first chunk should be Hello"
    );
    assert!(
        matches!(&chunks[3], StreamEvent::Done),
        "last chunk should be Done"
    );
    Ok(())
}

#[tokio::test]
async fn execute_tools_sends_tool_results_with_generation_id() -> Result<(), ShellError> {
    let (shell, mut input_rx, _system_rx, _async_rx, _gov_rx) = BriocheShell::test_with_channels();
    let executor = echo_executor();

    let calls = vec![ActiveToolCall {
        tool_id: "call_1".into(),
        tool_name: "echo".into(),
        arguments: "{\"msg\":\"hi\"}".into(),
        timeout_ms: 5000,
    }];

    executor.execute_tools(calls, 42, &shell).await?;

    let result = input_rx.recv().await.ok_or_else(|| {
        ShellError::EffectExecution("channel closed before ToolCallsResult".into())
    })?;
    match result {
        EngineInput::ToolCallsResult {
            generation_id,
            results,
        } => {
            assert_eq!(generation_id, 42);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].tool_id, "call_1");
            assert!(
                matches!(
                    &results[0].outcome, ToolOutcome::Success(s) if s == "{\"msg\":\"hi\"}"
                ),
                "echo tool should return arguments as success"
            );
        }
        other => {
            return Err(ShellError::EffectExecution(format!(
                "expected ToolCallsResult, got {other:?}"
            )));
        }
    }
    Ok(())
}

#[tokio::test]
async fn forward_to_ui_invokes_callback() -> Result<(), ShellError> {
    let received = Arc::new(Mutex::new(None));
    let received_clone = Arc::clone(&received);
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
            .with_ui_forwarder(move |widget: UiWidget| {
                let _ = received_clone.lock().map(|mut guard| *guard = Some(widget));
            });

    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();
    let widget = UiWidget::Status("ok".into());
    executor.forward_to_ui(widget.clone()).await?;

    assert!(
        received.lock().is_ok_and(|guard| *guard == Some(widget)),
        "UI forwarder should receive the widget"
    );
    Ok(())
}

#[tokio::test]
async fn log_error_does_not_fail() -> Result<(), ShellError> {
    let executor = echo_executor();
    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();

    executor
        .log_error(
            ErrorCode::NetworkUnavailable,
            ErrorDetail::TransitionFailed {
                reason: "test".into(),
            },
        )
        .await
}

#[tokio::test]
async fn save_session_async_dispatches_persistence() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    )
    .with_persistence_mode(PersistenceMode::Async);
    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();

    executor.save_session("session-x").await?;

    // Wait for the background task to run.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        persistence.save_session_count.load(Ordering::SeqCst),
        1,
        "persistence should have saved the session"
    );
    Ok(())
}

#[tokio::test]
async fn save_session_sync_blocks_on_persistence() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    )
    .with_persistence_mode(PersistenceMode::Sync);
    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();

    executor.save_session("session-y").await?;

    assert_eq!(
        persistence.save_session_count.load(Ordering::SeqCst),
        1,
        "sync save_session should complete persistence before returning"
    );
    Ok(())
}

#[tokio::test]
async fn save_plugin_blob_persists_blob() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    );
    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();

    executor.save_plugin_blob("plugin-a", vec![9, 8, 7]).await?;

    assert_eq!(
        persistence.save_blob_count.load(Ordering::SeqCst),
        1,
        "persistence should have saved the blob"
    );
    Ok(())
}

#[tokio::test]
async fn trigger_summarization_emits_async_result() -> Result<(), ShellError> {
    let (shell, _input_rx, _system_rx, mut async_rx, _gov_rx) = BriocheShell::test_with_channels();
    let history = Arc::new(RwLock::new(vec![
        ChatMessage::User {
            content: "a".into(),
        },
        ChatMessage::User {
            content: "b".into(),
        },
        ChatMessage::User {
            content: "c".into(),
        },
        ChatMessage::User {
            content: "d".into(),
        },
        ChatMessage::User {
            content: "e".into(),
        },
    ]));
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
            .with_history(Arc::clone(&history));

    executor.trigger_summarization(&shell).await?;

    let result = async_rx.recv().await.ok_or_else(|| {
        ShellError::EffectExecution("channel closed before SummarizationDone".into())
    })?;
    assert!(
        matches!(
            result,
            AsyncTaskResult::SummarizationDone {
                summary: ChatMessage::System { ref content },
                watermark: 3,
            } if content == "Mock summary of 3 messages"
        ),
        "expected summarization result for 3 messages, got {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn execute_cpu_task_dispatches_registered_handler() -> Result<(), ShellError> {
    let (shell, _input_rx, _system_rx, mut async_rx, _gov_rx) = BriocheShell::test_with_channels();

    let mut registry = CpuTaskRegistry::new();
    registry.register("double", |payload: &[u8]| {
        Ok(payload.iter().flat_map(|&b| [b, b]).collect())
    });
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
            .with_cpu_task_registry(Arc::new(registry));

    executor
        .execute_cpu_task("double".into(), vec![1, 2, 3], &shell)
        .await?;

    let result = async_rx
        .recv()
        .await
        .ok_or_else(|| ShellError::EffectExecution("channel closed before CpuTaskDone".into()))?;
    assert_eq!(
        result,
        AsyncTaskResult::CpuTaskDone {
            task_id: "double".into(),
            result: vec![1, 1, 2, 2, 3, 3],
        }
    );
    Ok(())
}

#[tokio::test]
async fn execute_cpu_task_falls_back_to_identity() -> Result<(), ShellError> {
    let (shell, _input_rx, _system_rx, mut async_rx, _gov_rx) = BriocheShell::test_with_channels();
    let executor = echo_executor();

    executor
        .execute_cpu_task("unknown".into(), vec![4, 5, 6], &shell)
        .await?;

    let result = async_rx
        .recv()
        .await
        .ok_or_else(|| ShellError::EffectExecution("channel closed before CpuTaskDone".into()))?;
    assert_eq!(
        result,
        AsyncTaskResult::CpuTaskDone {
            task_id: "unknown".into(),
            result: vec![4, 5, 6],
        }
    );
    Ok(())
}

#[tokio::test]
async fn trigger_gc_calls_persistence() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    );
    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();

    executor.trigger_gc("session-a").await?;

    assert_eq!(
        persistence.gc_count.load(Ordering::SeqCst),
        1,
        "gc should have been invoked once"
    );
    Ok(())
}

#[tokio::test]
async fn on_system_idle_does_not_trigger_gc() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    );
    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();

    executor.on_system_idle(&_shell, "session-b").await?;

    assert_eq!(
        persistence.gc_count.load(Ordering::SeqCst),
        0,
        "on_system_idle should not trigger GC"
    );
    Ok(())
}

#[tokio::test]
async fn rebuild_routes_is_no_op() -> Result<(), ShellError> {
    let executor = echo_executor();
    executor.rebuild_routes().await
}

#[tokio::test]
async fn sub_routine_restored_invokes_callback() -> Result<(), ShellError> {
    let received = Arc::new(Mutex::new(None));
    let received_clone = Arc::clone(&received);
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
            .with_subroutine_restored_callback(move |handle: SubRoutineHandle| {
                let _ = received_clone
                    .lock()
                    .map(|mut guard| *guard = Some(handle.as_str().to_string()));
            });

    let (_shell, _rx, _srx, _arx, _grx) = BriocheShell::test_with_channels();
    let handle = SubRoutineHandle::new("sub-42");
    executor.sub_routine_restored(handle.clone()).await?;

    assert!(
        received
            .lock()
            .is_ok_and(|guard| *guard == Some("sub-42".to_string())),
        "subroutine restored callback should receive the handle"
    );
    Ok(())
}

#[tokio::test]
async fn call_llm_network_emits_system_signal_on_failure() -> Result<(), ShellError> {
    use brioche_shell_runtime::network_recovery::NoRetry;

    struct FailingLlmClient;

    #[async_trait::async_trait]
    impl brioche_shell_runtime::LlmClient for FailingLlmClient {
        async fn call_llm(&self, _shell: &BriocheShell) -> Result<(), ShellError> {
            Err(ShellError::EffectExecution("boom".into()))
        }

        async fn summarize(
            &self,
            _shell: &BriocheShell,
            _messages: &[ChatMessage],
        ) -> Result<ChatMessage, ShellError> {
            Ok(ChatMessage::System {
                content: "summary".into(),
            })
        }

        async fn push_tool_results(&self, _results: &[ToolResultDTO]) {}
    }

    let (shell, mut input_rx, mut system_rx, _async_rx, _gov_rx) =
        BriocheShell::test_with_channels();
    let executor = DefaultEffectExecutor::new(EchoToolExecutor, FailingLlmClient, NoopPersistence)
        .with_network_recovery(NoRetry);

    executor.call_llm(&shell).await?;

    let signal = system_rx
        .recv()
        .await
        .ok_or_else(|| ShellError::EffectExecution("no system signal emitted".into()))?;
    assert!(
        matches!(
            signal,
            SystemSignal::NetworkUnavailable { ref reason } if reason.contains("boom")
        ),
        "expected NetworkUnavailable with reason containing 'boom', got {signal:?}"
    );

    // No LlmStream inputs should be produced.
    assert!(
        input_rx.try_recv().is_err(),
        "no engine inputs should be sent on failure"
    );
    Ok(())
}
