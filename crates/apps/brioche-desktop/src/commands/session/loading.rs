//! Persisted session loading commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_core::ChatMessage;
use tauri::AppHandle;

use super::chat::emit_system;
use crate::state::{DesktopState, SessionMetadata, persist_session};

pub(super) async fn load_session(
    app: &AppHandle,
    state: &DesktopState,
    id: &str,
) -> Result<(), String> {
    match load_session_impl(state, id).await {
        Ok(messages) => {
            emit_system(
                app,
                format!("Session '{}' loaded ({} messages).", id, messages.len()),
            );
            Ok(())
        }
        Err(message) => {
            emit_system(app, message.clone());
            Err(message)
        }
    }
}

/// Implementation of [`load_session`] that does not need a Tauri
/// [`AppHandle`], so it can be exercised from library tests.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S + M) where S is the number of sessions and M is memory provider initialization.
///
/// # Panic / Safety
/// Never panics. Returns Err if the session is not found or shell rebuild fails.
pub(super) async fn load_session_impl(
    state: &DesktopState,
    id: &str,
) -> Result<Vec<ChatMessage>, String> {
    state.ensure_manager().await?;
    let factory = state.factory.read().await.clone();
    match factory.redb.load_session(id).await {
        Ok(None) => return Err(format!("Session '{}' not found.", id)),
        Err(err) => return Err(format!("Load error: {err}")),
        Ok(Some(_)) => {}
    }
    let messages: Vec<ChatMessage> = match factory.redb.load_messages_for_session(id).await {
        Ok(msgs) => msgs.into_iter().map(|(_, m)| m).collect(),
        Err(err) => return Err(format!("Load messages error: {err}")),
    };
    let workspace = factory.settings.working_dir();
    let handle = crate::commands::shell::build_shell(id, &factory)
        .await
        .map_err(|e| e.to_string())?;
    DesktopState::initialize_memory_providers(&factory, id, &workspace)?;
    for msg in &messages {
        handle.llm.push_message(msg.clone()).await;
    }
    {
        let mut mgr = state.manager.write().await;
        let manager = mgr.as_mut().ok_or("No active session")?;
        manager.insert(
            id.to_string(),
            handle.shell,
            handle.llm,
            handle.history,
            handle.llm_rx,
        );
        if !matches!(manager.metadata(id), Some(meta) if meta.created_at != 0) {
            manager.insert_metadata(SessionMetadata::new(id, &workspace))?;
        }
        manager.switch(id);
    }
    persist_session(state).await?;
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::session::test_support::test_state;

    #[tokio::test]
    async fn load_session_impl_errors_for_missing_session() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        state.ensure_manager().await?;
        let result = load_session_impl(&state, "nonexistent-session").await;
        let err = match result {
            Err(e) => e,
            Ok(_) => return Err("expected error for missing session".into()),
        };
        assert!(
            err.contains("not found") || err.contains("does not exist"),
            "expected missing-session error, got: {err}"
        );
        Ok(())
    }
}
