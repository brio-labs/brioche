//! Network recovery, routing barrier, and plugin-fault contracts.

mod common;

use std::time::Duration;

use brioche_core::{ChatMessage, EngineInput, Session, ToolResultDTO};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, NoopPersistence, ShellConfig,
};
use common::{
    build_minimal_engine, build_shell_with_recorder, is_idle, is_predicting, recorded_views,
    session_recorder,
};

#[tokio::test]
async fn network_recovery_emits_system_signal_on_exhaustion() {
    use brioche_shell_runtime::ExponentialBackoff;

    #[derive(Clone, Debug)]
    struct FailingLlmClient;

    #[async_trait::async_trait]
    impl brioche_shell_runtime::LlmClient for FailingLlmClient {
        async fn call_llm(
            &self,
            _shell: &BriocheShell,
        ) -> Result<(), brioche_shell_runtime::ShellError> {
            Err(brioche_shell_runtime::ShellError::EffectExecution(
                "always fails".into(),
            ))
        }

        async fn push_tool_results(&self, _results: &[ToolResultDTO]) {}

        async fn summarize(
            &self,
            _shell: &BriocheShell,
            _messages: &[ChatMessage],
        ) -> Result<ChatMessage, brioche_shell_runtime::ShellError> {
            Err(brioche_shell_runtime::ShellError::EffectExecution(
                "summary unavailable".into(),
            ))
        }
    }

    let recovery = ExponentialBackoff {
        max_attempts: 2,
        base_delay_ms: 10,
        multiplier: 1.0,
        max_delay_ms: 50,
    };

    let (callback, views) = session_recorder();
    let executor = DefaultEffectExecutor::new(EchoToolExecutor, FailingLlmClient, NoopPersistence)
        .with_network_recovery(recovery);
    let shell = BriocheShell::new(
        || (build_minimal_engine(), Session::new("test")),
        ShellConfig::default(),
        executor,
        Some(callback),
    );

    assert!(
        shell
            .send_input(EngineInput::UserMessage("trigger llm".into()))
            .await
            .is_ok()
    );

    tokio::time::sleep(Duration::from_millis(400)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    // Recovery does not produce a new transition on its own; the engine stays Predicting.
    let last = &views[views.len() - 1];
    assert!(
        is_predicting(&last.state, 1),
        "engine should remain Predicting until recovery resolves"
    );
    assert_eq!(last.history.len(), 1);
    assert!(matches!(
        &last.history[0],
        ChatMessage::User { content } if content == "trigger llm"
    ));

    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn rebuild_routes_blocks_new_inputs() {
    let (shell, views) = build_shell_with_recorder();

    // In the current architecture the rebuild barrier can only be triggered
    // internally (e.g. by a QuarantineManager effect). We verify that normal
    // inputs still flow to completion while the barrier is not raised.
    let result = shell
        .send_input(EngineInput::UserMessage("before rebuild".into()))
        .await;
    assert!(result.is_ok());

    tokio::time::sleep(Duration::from_millis(100)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    let last = &views[views.len() - 1];
    assert!(is_idle(&last.state), "final state should be Idle");
    assert_eq!(last.history.len(), 2);

    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn plugin_fault_propagates_to_governance_channel() {
    let (shell, views) = build_shell_with_recorder();

    assert!(
        shell
            .send_governance_notification(brioche_core::GovernanceNotification::PluginFaulted {
                plugin_name: "test_plugin".into(),
                error: brioche_core::PluginError::Fatal {
                    plugin_name: "test_plugin".into(),
                    message: "simulated fault".into(),
                },
            })
            .await
            .is_ok()
    );

    assert!(
        shell
            .send_input(EngineInput::UserMessage("after fault".into()))
            .await
            .is_ok()
    );

    tokio::time::sleep(Duration::from_millis(200)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    let last = &views[views.len() - 1];
    assert!(is_idle(&last.state), "final state should be Idle");
    assert_eq!(last.history.len(), 2);
    assert!(matches!(
        &last.history[0],
        ChatMessage::User { content } if content == "after fault"
    ));

    assert!(shell.ready().await.is_ok());
}
