//! Effect execution and shell-owned task shutdown contracts.

mod common;

use std::sync::Arc;
use std::time::Duration;

use brioche_core::{
    ActiveToolCall, BriocheEngineBuilder, ChatMessage, Effect, EngineInput, ExtensionStorage,
    OnInput, PluginError, PluginResult, PolicyDecision, Session, ToolResultDTO,
};
use brioche_governance_default::{LexicographicDecisionAggregator, SubRoutineCleanupGuard};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence,
    Persistence, ShellConfig, ShellError, ToolExecutor,
};
use common::{
    build_minimal_engine, is_executing_tools, is_predicting, recorded_views, session_recorder,
};
use tokio::sync::{Notify, oneshot};

struct SaveSessionOnInput;

impl OnInput for SaveSessionOnInput {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "save_session_on_input"
    }

    fn priority(&self) -> i16 {
        0
    }

    fn on_input(
        &self,
        input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        if matches!(input, EngineInput::UserMessage(_)) {
            Ok(PolicyDecision::RequestEffect(Effect::SaveSession))
        } else {
            Ok(PolicyDecision::Allow)
        }
    }
}

#[derive(Clone)]
struct BlockingSavePersistence {
    started: Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>>,
    release: Arc<Notify>,
}

impl BlockingSavePersistence {
    fn new(started: oneshot::Sender<()>) -> Self {
        Self {
            started: Arc::new(std::sync::Mutex::new(Some(started))),
            release: Arc::new(Notify::new()),
        }
    }

    fn release(&self) {
        self.release.notify_waiters();
    }
}

#[async_trait::async_trait]
impl Persistence for BlockingSavePersistence {
    async fn save_session(&self, _session_id: &str) -> Result<(), ShellError> {
        if let Ok(mut guard) = self.started.lock()
            && let Some(started) = guard.take()
        {
            let _ = started.send(());
        }
        self.release.notified().await;
        Ok(())
    }

    async fn save_plugin_blob(&self, _plugin_id: &str, _data: Vec<u8>) -> Result<(), ShellError> {
        Ok(())
    }

    async fn gc(&self, _session_id: &str) -> Result<u64, ShellError> {
        Ok(0)
    }
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
    /// LLM client that does nothing, letting the test drive the stream manually.
    #[derive(Clone, Debug, Default)]
    struct NoopLlmClient;

    #[async_trait::async_trait]
    impl brioche_shell_runtime::LlmClient for NoopLlmClient {
        async fn call_llm(
            &self,
            _shell: &BriocheShell,
        ) -> Result<(), brioche_shell_runtime::ShellError> {
            Ok(())
        }

        async fn push_tool_results(&self, _results: &[ToolResultDTO]) {}

        async fn summarize(
            &self,
            _shell: &BriocheShell,
            _messages: &[ChatMessage],
        ) -> Result<ChatMessage, brioche_shell_runtime::ShellError> {
            Ok(ChatMessage::System {
                content: "noop summary".into(),
            })
        }
    }

    let (callback, views) = session_recorder();
    let executor = DefaultEffectExecutor::new(EchoToolExecutor, NoopLlmClient, NoopPersistence);
    let shell = BriocheShell::new(
        || (build_minimal_engine(), Session::new("test")),
        ShellConfig::default(),
        executor,
        Some(callback),
    );

    // Start a prediction.
    assert!(
        shell
            .send_input(EngineInput::UserMessage("call tools".into()))
            .await
            .is_ok()
    );

    // Inject a tool call stream.
    assert!(
        shell
            .send_input(EngineInput::LlmStream(
                brioche_core::StreamEvent::ToolCallStart {
                    path: Default::default(),
                    id: "tc1".into(),
                    name: "calc".into(),
                }
            ))
            .await
            .is_ok()
    );
    assert!(
        shell
            .send_input(EngineInput::LlmStream(
                brioche_core::StreamEvent::ToolArgumentChunk {
                    path: Default::default(),
                    id: "tc1".into(),
                    chunk: bytes::Bytes::from_static(b"{\"x\":1}"),
                }
            ))
            .await
            .is_ok()
    );
    assert!(
        shell
            .send_input(EngineInput::LlmStream(
                brioche_core::StreamEvent::ToolCallDone {
                    path: Default::default(),
                }
            ))
            .await
            .is_ok()
    );

    // Wait for the ExecuteTools effect and the ToolCallsResult loopback.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // End the prediction.
    assert!(
        shell
            .send_input(EngineInput::LlmStream(brioche_core::StreamEvent::Done))
            .await
            .is_ok()
    );

    tokio::time::sleep(Duration::from_millis(100)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    // Verify the effect loop did not drop or reorder the tool-execution transition.
    assert!(
        views.iter().any(|v| is_executing_tools(&v.state, 1)),
        "should have entered ExecutingTools after ToolCallDone"
    );

    let last = &views[views.len() - 1];
    assert!(
        is_predicting(&last.state, 1),
        "final state should be Predicting with generation_id 1, got {:?}",
        last.state
    );
    assert_eq!(
        last.generation_id,
        Some(1),
        "generation_id should stay consistent"
    );
    assert_eq!(last.history.len(), 2);
    assert!(
        matches!(
            &last.history[0],
            ChatMessage::User { content } if content == "call tools"
        ),
        "user message should be preserved"
    );
    assert!(
        matches!(
            &last.history[1],
            ChatMessage::ToolResult { id, content } if id == "tc1" && content == "{\"x\":1}"
        ),
        "tool result should be injected into history"
    );
}

#[tokio::test]
async fn shell_shutdown_awaits_in_flight_effect_tasks_and_rejects_later_inputs() {
    let (started_tx, started_rx) = oneshot::channel();
    let persistence = BlockingSavePersistence::new(started_tx);
    let executor = DefaultEffectExecutor::new(
        EchoToolExecutor,
        MockLlmClient::default(),
        persistence.clone(),
    );
    let shell = BriocheShell::new(
        || {
            let engine = BriocheEngineBuilder::new()
                .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
                .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard))
                .with_on_input(Box::new(SaveSessionOnInput))
                .build();
            (engine, Session::new("shutdown-awaits-effects"))
        },
        ShellConfig::default(),
        executor,
        None,
    );

    assert!(
        shell
            .send_input(EngineInput::UserMessage("persist before shutdown".into()))
            .await
            .is_ok()
    );
    let save_started = tokio::time::timeout(Duration::from_millis(500), started_rx).await;
    assert!(
        matches!(save_started, Ok(Ok(()))),
        "SaveSession effect should start before shutdown"
    );

    let shell_for_shutdown = shell.clone();
    let mut shutdown = tokio::spawn(async move {
        shell_for_shutdown.shutdown().await;
    });

    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut shutdown)
            .await
            .is_err(),
        "shutdown must wait for the shell-owned save task"
    );

    persistence.release();

    let shutdown_result = tokio::time::timeout(Duration::from_millis(500), shutdown).await;
    assert!(
        matches!(shutdown_result, Ok(Ok(()))),
        "shutdown should complete after the in-flight effect task completes"
    );
    assert!(
        shell
            .send_input(EngineInput::UserMessage("after shutdown".into()))
            .await
            .is_err(),
        "shutdown should make the shell reject later inputs"
    );
}
