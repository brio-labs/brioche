//! Transition journal and persistence-mode contracts.

mod common;

use std::time::Duration;

use brioche_core::{ChatMessage, EngineInput, Session};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence,
    ShellConfig,
};

use common::{
    build_minimal_engine, build_shell_with_recorder, is_idle, recorded_views, session_recorder,
};

#[tokio::test]
async fn transition_journal_persists_inputs() {
    let (shell, views) = build_shell_with_recorder();

    assert!(
        shell
            .send_input(EngineInput::UserMessage("first".into()))
            .await
            .is_ok()
    );
    assert!(
        shell
            .send_input(EngineInput::UserMessage("second".into()))
            .await
            .is_ok()
    );

    tokio::time::sleep(Duration::from_millis(200)).await;

    let views = recorded_views(&views);
    assert!(!views.is_empty(), "expected transition snapshots");

    let last = &views[views.len() - 1];
    assert!(is_idle(&last.state), "final state should be Idle");
    assert_eq!(
        last.history.len(),
        4,
        "both user messages and both assistant replies should be present"
    );
    assert!(matches!(
        &last.history[0],
        ChatMessage::User { content } if content == "first"
    ));
    assert!(matches!(
        &last.history[1],
        ChatMessage::User { content } if content == "second"
    ));
    assert!(matches!(
        &last.history[2],
        ChatMessage::Assistant { content, .. } if content == "Hello world"
    ));
    assert!(matches!(
        &last.history[3],
        ChatMessage::Assistant { content, .. } if content == "Hello world"
    ));

    assert!(shell.ready().await.is_ok());
}

#[tokio::test]
async fn persistence_mode_sync_blocks_on_save() {
    use brioche_shell_runtime::PersistenceMode;

    let (callback, views) = session_recorder();
    let config = ShellConfig {
        persistence_mode: PersistenceMode::Sync,
        ..Default::default()
    };
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
            .with_persistence_mode(PersistenceMode::Sync);

    let shell = BriocheShell::new(
        || (build_minimal_engine(), Session::new("test")),
        config,
        executor,
        Some(callback),
    );

    assert!(
        shell
            .send_input(EngineInput::UserMessage("sync save".into()))
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
        ChatMessage::User { content } if content == "sync save"
    ));

    assert!(shell.ready().await.is_ok());
}
