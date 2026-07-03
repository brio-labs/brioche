//! Integration tests for `DefaultEffectExecutor`.
//!
//! Exercises every `Effect` variant through the default executor using
//! `MockLlmClient`, `EchoToolExecutor`, and a local counting `Persistence`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use brioche_core::{
    ActiveToolCall, AsyncTaskResult, ChatMessage, EngineInput, ErrorCode, ErrorDetail, StreamEvent,
    SubRoutineHandle, ToolOutcome, UiWidget,
};
use bytes::Bytes;
use tokio::sync::mpsc;

use brioche_shell_runtime::effect_executor::CpuTaskRegistry;
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, EffectExecutor, MockLlmClient,
    NoopPersistence, Persistence, PersistenceMode, ShellError,
};

// ---------------------------------------------------------------------------
// Local counting persistence
// ---------------------------------------------------------------------------

/// A test persistence layer that records every method call.
#[derive(Clone, Default)]
struct CountingPersistence {
    save_session_count: Arc<AtomicU64>,
    save_plugin_blob_count: Arc<AtomicU64>,
    gc_count: Arc<AtomicU64>,
    saved_sessions: Arc<Mutex<Vec<String>>>,
    saved_blobs: Arc<Mutex<Vec<BlobRecord>>>,
}

type BlobRecord = (String, Vec<u8>);

#[async_trait]
impl Persistence for CountingPersistence {
    async fn save_session(&self, session_id: &str) -> Result<(), ShellError> {
        self.save_session_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.saved_sessions.lock() {
            guard.push(session_id.to_string());
        }
        Ok(())
    }

    async fn save_plugin_blob(&self, plugin_id: &str, data: Vec<u8>) -> Result<(), ShellError> {
        self.save_plugin_blob_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = self.saved_blobs.lock() {
            guard.push((plugin_id.to_string(), data));
        }
        Ok(())
    }

    async fn gc(&self, _session_id: &str) -> Result<u64, ShellError> {
        self.gc_count.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Effect variant tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn effect_call_llm_network_streams_text_chunks() -> Result<(), ShellError> {
    let (input_tx, mut input_rx) = mpsc::channel(16);
    let (async_tx, _async_rx) = mpsc::channel(1);
    let shell = BriocheShell::test_with_loopback_channels(input_tx, async_tx);
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);

    executor.call_llm(&shell).await?;

    let mut chunks = Vec::new();
    let mut saw_done = false;
    while let Some(input) = input_rx.recv().await {
        match input {
            EngineInput::LlmStream(StreamEvent::TextChunk { chunk, .. }) => {
                chunks.push(chunk);
            }
            EngineInput::LlmStream(StreamEvent::Done) => {
                saw_done = true;
                break;
            }
            _ => {}
        }
    }

    assert!(saw_done, "expected Done marker");
    assert_eq!(
        chunks,
        vec![Bytes::from("Hello"), Bytes::from(" "), Bytes::from("world")]
    );
    Ok(())
}

#[tokio::test]
async fn effect_execute_tools_returns_tool_calls_result() -> Result<(), ShellError> {
    let (input_tx, mut input_rx) = mpsc::channel(16);
    let (async_tx, _async_rx) = mpsc::channel(1);
    let shell = BriocheShell::test_with_loopback_channels(input_tx, async_tx);
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);

    let calls = vec![ActiveToolCall {
        tool_id: "tc1".into(),
        tool_name: "echo".into(),
        arguments: "{\"msg\":\"hi\"}".into(),
        timeout_ms: 1000,
    }];

    executor.execute_tools(calls, 42, &shell).await?;

    let result = input_rx
        .recv()
        .await
        .ok_or_else(|| ShellError::EffectExecution("expected ToolCallsResult".into()))?;
    let EngineInput::ToolCallsResult {
        generation_id,
        results,
    } = result
    else {
        return Err(ShellError::EffectExecution(format!(
            "expected ToolCallsResult, got {:?}",
            result
        )));
    };
    assert_eq!(generation_id, 42);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_id, "tc1");
    assert_eq!(results[0].tool_name, "echo");
    assert!(
        matches!(
            &results[0].outcome,
            ToolOutcome::Success(s) if s == "{\"msg\":\"hi\"}"
        ),
        "echo executor should return arguments as success"
    );
    Ok(())
}

#[tokio::test]
async fn effect_forward_to_ui_forwards_widget() -> Result<(), ShellError> {
    let received = Arc::new(Mutex::new(None));
    let received_clone = Arc::clone(&received);
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
            .with_ui_forwarder(move |widget: UiWidget| {
                let _ = received_clone.lock().map(|mut guard| *guard = Some(widget));
            });

    let widget = UiWidget::Status("ready".into());
    executor.forward_to_ui(widget.clone()).await?;

    let guard = received
        .lock()
        .map_err(|_| ShellError::EffectExecution("mutex poisoned".into()))?;
    assert_eq!(*guard, Some(widget));
    Ok(())
}

#[tokio::test]
async fn effect_error_logs_and_succeeds() -> Result<(), ShellError> {
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);

    executor
        .log_error(
            ErrorCode::StateInconsistency,
            ErrorDetail::TransitionFailed {
                reason: "test error".into(),
            },
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn effect_save_session_persists() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    )
    .with_persistence_mode(PersistenceMode::Sync);

    executor.save_session("session-abc").await?;

    assert_eq!(
        persistence.save_session_count.load(Ordering::SeqCst),
        1,
        "save_session should be called once"
    );
    let sessions = persistence
        .saved_sessions
        .lock()
        .map_err(|_| ShellError::EffectExecution("mutex poisoned".into()))?;
    assert_eq!(sessions.as_slice(), &["session-abc"]);
    Ok(())
}

#[tokio::test]
async fn effect_save_plugin_blob_persists() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    );

    let data = vec![1, 2, 3, 4];
    executor
        .save_plugin_blob("plugin-xyz", data.clone())
        .await?;

    assert_eq!(
        persistence.save_plugin_blob_count.load(Ordering::SeqCst),
        1,
        "save_plugin_blob should be called once"
    );
    let blobs = persistence
        .saved_blobs
        .lock()
        .map_err(|_| ShellError::EffectExecution("mutex poisoned".into()))?;
    assert_eq!(blobs.as_slice(), &[("plugin-xyz".into(), data)]);
    Ok(())
}

#[tokio::test]
async fn effect_trigger_summarization_emits_async_result() -> Result<(), ShellError> {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (async_tx, mut async_rx) = mpsc::channel(4);
    let shell = BriocheShell::test_with_loopback_channels(input_tx, async_tx);
    let history = Arc::new(tokio::sync::RwLock::new(vec![
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

    let result = async_rx
        .recv()
        .await
        .ok_or_else(|| ShellError::EffectExecution("expected SummarizationDone async task result".into()))?;
    assert!(
        matches!(
            result,
            AsyncTaskResult::SummarizationDone {
                summary: ChatMessage::System { ref content },
                watermark: 3,
            } if content == "Mock summary of 3 messages"
        ),
        "expected summarization of 3 messages, got {:?}",
        result
    );
    Ok(())
}

#[tokio::test]
async fn effect_execute_cpu_task_dispatches_registry() -> Result<(), ShellError> {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (async_tx, mut async_rx) = mpsc::channel(4);
    let shell = BriocheShell::test_with_loopback_channels(input_tx, async_tx);
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
        .ok_or_else(|| ShellError::EffectExecution("expected CpuTaskDone async task result".into()))?;
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
async fn effect_trigger_gc_calls_persistence() -> Result<(), ShellError> {
    let persistence = CountingPersistence::default();
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    );
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (async_tx, _async_rx) = mpsc::channel(1);
    let _shell = BriocheShell::test_with_loopback_channels(input_tx, async_tx);

    executor.trigger_gc("session-gc").await?;

    assert_eq!(
        persistence.gc_count.load(Ordering::SeqCst),
        1,
        "gc should be invoked once"
    );
    Ok(())
}

#[tokio::test]
async fn effect_system_idle_is_no_op() -> Result<(), ShellError> {
    let (input_tx, _input_rx) = mpsc::channel(1);
    let (async_tx, _async_rx) = mpsc::channel(1);
    let shell = BriocheShell::test_with_loopback_channels(input_tx, async_tx);
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);

    executor.on_system_idle(&shell, "session-idle").await?;
    Ok(())
}

#[tokio::test]
async fn effect_rebuild_routes_is_no_op() -> Result<(), ShellError> {
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);

    executor.rebuild_routes().await?;
    Ok(())
}

#[tokio::test]
async fn effect_sub_routine_restored_invokes_callback() -> Result<(), ShellError> {
    let received = Arc::new(Mutex::new(None));
    let received_clone = Arc::clone(&received);
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
            .with_subroutine_restored_callback(move |handle: SubRoutineHandle| {
                let _ = received_clone
                    .lock()
                    .map(|mut guard| *guard = Some(handle.as_str().to_string()));
            });

    let handle = SubRoutineHandle::new("sub-42");
    executor.sub_routine_restored(handle).await?;

    let guard = received
        .lock()
        .map_err(|_| ShellError::EffectExecution("mutex poisoned".into()))?;
    assert_eq!(*guard, Some("sub-42".to_string()));
    Ok(())
}
