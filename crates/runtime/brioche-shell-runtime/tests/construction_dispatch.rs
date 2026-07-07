//! Shell construction, dispatch, startup, and shutdown contracts.

mod common;

use std::time::Duration;

use brioche_core::{AgentState, ChatMessage, EngineInput, SystemSignal};

use common::{build_shell, build_shell_with_recorder, is_idle, is_predicting, recorded_views};

#[tokio::test]
async fn shell_dispatches_user_message() {
    let (shell, views) = build_shell_with_recorder();

    assert!(
        shell
            .send_input(EngineInput::UserMessage("hello".into()))
            .await
            .is_ok(),
        "input send should succeed"
    );

    // Allow the engine thread to process the full mock stream.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    let first = &views[0];
    assert!(
        is_predicting(&first.state, 1),
        "first transition should enter Predicting"
    );
    assert_eq!(first.history.len(), 1);
    assert!(
        matches!(
            &first.history[0],
            ChatMessage::User { content } if content == "hello"
        ),
        "user message should be first history entry"
    );

    // Invariants: no lost transitions, generation id stays consistent.
    for view in &views {
        if let AgentState::Predicting { generation_id }
        | AgentState::ExecutingTools { generation_id } = view.state
        {
            assert_eq!(generation_id, 1, "generation_id should remain 1");
        }
    }

    let last = &views[views.len() - 1];
    assert!(is_idle(&last.state), "final state should be Idle");
    assert_eq!(
        last.history.len(),
        2,
        "final history should contain the user message and assistant reply"
    );
    assert!(
        matches!(
            &last.history[1],
            ChatMessage::Assistant { content, .. } if content == "Hello world"
        ),
        "assistant reply should be accumulated from streamed chunks"
    );

    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn shell_routes_system_signal() {
    let (shell, views) = build_shell_with_recorder();

    assert!(
        shell
            .send_system_signal(SystemSignal::OperationCancelled)
            .await
            .is_ok(),
        "signal send should succeed"
    );

    // Trigger a transition so the shell drains the signal buffer.
    assert!(
        shell
            .send_input(EngineInput::UserMessage("hello".into()))
            .await
            .is_ok(),
        "input send should succeed"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    let first = &views[0];
    assert!(
        is_predicting(&first.state, 1),
        "user message should still transition to Predicting"
    );
    assert_eq!(first.history.len(), 1);

    let last = &views[views.len() - 1];
    assert!(is_idle(&last.state), "final state should be Idle");
    assert_eq!(last.history.len(), 2);

    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn shell_graceful_shutdown() {
    let shell = build_shell();

    assert!(
        shell.ready().await.is_ok(),
        "shell should be ready before shutdown"
    );
    assert!(
        shell.health_check(),
        "shell should be healthy before shutdown"
    );

    shell.shutdown().await;

    assert!(
        shell.ready().await.is_err(),
        "shell should reject input readiness after shutdown"
    );
    assert!(
        shell.health_check(),
        "shutdown should leave no unhealthy tracked task handles"
    );
}

#[tokio::test]
async fn shell_startup_procedure_completes() {
    let (shell, views) = build_shell_with_recorder();

    assert!(
        shell
            .send_input(EngineInput::UserMessage("startup ok".into()))
            .await
            .is_ok()
    );

    tokio::time::sleep(Duration::from_millis(100)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    let last = &views[views.len() - 1];
    assert!(is_idle(&last.state), "final state should be Idle");
    assert_eq!(last.history.len(), 2);

    assert!(shell.ready().await.is_ok());
}
