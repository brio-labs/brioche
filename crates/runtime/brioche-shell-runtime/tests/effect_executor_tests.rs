//! Tests for effect_executor infrastructure.
//!
//! Covers `NoopPersistence` and `DefaultEffectExecutor` construction.

use brioche_shell_runtime::effect_executor::{NoopPersistence, Persistence};

#[tokio::test]
async fn noop_persistence_save_session_succeeds() {
    let persistence = NoopPersistence;
    let result = persistence.save_session("test-session").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn noop_persistence_save_plugin_blob_succeeds() {
    let persistence = NoopPersistence;
    let data = vec![1, 2, 3, 4];
    let result = persistence.save_plugin_blob("test-plugin", data).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn noop_persistence_is_default() {
    let persistence: NoopPersistence = Default::default();
    let result = persistence.save_session("default-test").await;
    assert!(result.is_ok());
}
