//! Session creation, switching, deletion, and persistence commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_shell_runtime::util::system_time_secs;
use tauri::{AppHandle, Emitter, State};

use super::chat::emit_system;
use crate::state::{DesktopState, SessionMetadata, persist_session};

/// Switches to an existing session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S + M) where S is the number of sessions and M is memory provider initialization.
///
/// # Panic / Safety
/// Never panics. Returns Err if the session is not found or memory provider initialization fails.
pub async fn switch_session(
    app: AppHandle,
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    state.ensure_manager().await?;
    let factory = state.factory.read().await.clone();
    {
        let mut mgr = state.manager.write().await;
        let manager = mgr.as_mut().ok_or("No active session")?;
        if !manager.list().iter().any(|sid| sid == &&id) {
            return Err(format!("Session '{}' not found", id));
        }
        manager.switch(&id);
    }
    let workspace = factory.settings.working_dir();
    DesktopState::initialize_memory_providers(&factory, &id, &workspace)?;
    persist_session(state.inner()).await?;
    emit_system(&app, format!("Switched to session: {id}"));
    let _ = app.emit("session-changed", id);
    Ok(())
}

/// Deletes a session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(log S) deletion where S is the number of active sessions. Saves changes to disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if the session is active, missing, or metadata cannot be saved.
pub async fn delete_session(
    app: AppHandle,
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    state.ensure_manager().await?;
    let mut mgr = state.manager.write().await;
    let manager = mgr.as_mut().ok_or("No active session")?;
    manager.delete_non_current(&id)?;
    drop(mgr);
    let _ = app.emit("sessions-updated", ());
    Ok(())
}

pub(super) async fn new_session_impl(state: &DesktopState) -> Result<String, String> {
    state.ensure_manager().await?;
    let new_id = format!("session-{}", system_time_secs());
    let factory = state.factory.read().await.clone();
    let workspace = factory.settings.working_dir();
    let handle = crate::commands::shell::build_shell(&new_id, &factory)
        .await
        .map_err(|e| e.to_string())?;
    DesktopState::initialize_memory_providers(&factory, &new_id, &workspace)?;
    {
        let mut mgr = state.manager.write().await;
        let manager = mgr.as_mut().ok_or("No active session")?;
        manager.insert(
            new_id.clone(),
            handle.shell,
            handle.llm,
            handle.history,
            handle.llm_rx,
        );
        manager.insert_metadata(SessionMetadata::new(&new_id, &workspace))?;
        manager.switch(&new_id);
    }
    persist_session(state).await?;
    Ok(new_id)
}

/// Creates a new session and switches to it.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S + M) where S is shell creation and M is memory provider initialization.
///
/// # Panic / Safety
/// Never panics. Returns Err if shell build, memory provider initialization, or metadata save fails.
pub async fn new_session(app: AppHandle, state: State<'_, DesktopState>) -> Result<String, String> {
    let id = new_session_impl(state.inner()).await?;
    let _ = app.emit("session-changed", id.clone());
    let _ = app.emit("sessions-updated", ());
    Ok(id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use brioche_core::ChatMessage;
    use brioche_shell_persistence::{
        FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion, SessionStoreEntry,
    };

    use super::*;
    use crate::commands::session::test_support::test_state;

    #[tokio::test]
    async fn delete_non_current_rejects_current_session() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        state.ensure_manager().await?;

        let current_id = {
            let mgr = state.manager.read().await;
            let manager = match mgr.as_ref() {
                Some(manager) => manager,
                None => return Err("manager should exist after ensure_manager".into()),
            };
            manager.current_id().to_string()
        };

        let result = {
            let mut mgr = state.manager.write().await;
            let manager = match mgr.as_mut() {
                Some(manager) => manager,
                None => return Err("manager should exist for active-session deletion".into()),
            };
            manager.delete_non_current(&current_id)
        };
        let err = match result {
            Err(err) => err,
            Ok(()) => return Err("expected active-session deletion to fail".into()),
        };
        if err != "Cannot delete the active session" {
            return Err(format!("expected active-session error, got: {err}"));
        }

        let mgr = state.manager.read().await;
        let manager = match mgr.as_ref() {
            Some(manager) => manager,
            None => return Err("manager should remain after rejected deletion".into()),
        };
        if manager.current_id() != current_id {
            return Err(format!(
                "current session changed after rejected deletion: expected {current_id}, got {}",
                manager.current_id()
            ));
        }
        if manager.get(&current_id).is_none() {
            return Err("current session missing after rejected deletion".into());
        }

        Ok(())
    }

    #[tokio::test]
    async fn delete_non_current_rejects_missing_session() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        state.ensure_manager().await?;

        let current_id = {
            let mgr = state.manager.read().await;
            let manager = match mgr.as_ref() {
                Some(manager) => manager,
                None => return Err("manager should exist after ensure_manager".into()),
            };
            manager.current_id().to_string()
        };

        let missing_id = "missing-session";
        let result = {
            let mut mgr = state.manager.write().await;
            let manager = match mgr.as_mut() {
                Some(manager) => manager,
                None => return Err("manager should exist for missing-session deletion".into()),
            };
            manager.delete_non_current(missing_id)
        };
        let err = match result {
            Err(err) => err,
            Ok(()) => return Err("expected missing-session deletion to fail".into()),
        };
        if err != "Session 'missing-session' not found" {
            return Err(format!("expected missing-session error, got: {err}"));
        }

        let mgr = state.manager.read().await;
        let manager = match mgr.as_ref() {
            Some(manager) => manager,
            None => return Err("manager should remain after missing-session deletion".into()),
        };
        if manager.current_id() != current_id {
            return Err(format!(
                "current session changed after missing-session deletion: expected {current_id}, got {}",
                manager.current_id()
            ));
        }

        Ok(())
    }

    #[tokio::test]
    async fn delete_non_current_removes_session_and_keeps_current() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        state.ensure_manager().await?;

        let non_current_id = {
            let mgr = state.manager.read().await;
            let manager = match mgr.as_ref() {
                Some(manager) => manager,
                None => return Err("manager should exist after ensure_manager".into()),
            };
            manager.current_id().to_string()
        };
        let current_id = "delete-non-current-kept-current".to_string();
        let factory = state.factory.read().await.clone();
        let workspace = factory.settings.working_dir();
        let handle = crate::commands::shell::build_shell(&current_id, &factory)
            .await
            .map_err(|err| err.to_string())?;
        DesktopState::initialize_memory_providers(&factory, &current_id, &workspace)?;

        {
            let mut mgr = state.manager.write().await;
            let manager = match mgr.as_mut() {
                Some(manager) => manager,
                None => return Err("manager should exist for non-current deletion".into()),
            };
            manager.insert(
                current_id.clone(),
                handle.shell,
                handle.llm,
                handle.history,
                handle.llm_rx,
            );
            manager.insert_metadata(SessionMetadata::new(&current_id, &workspace))?;
            manager.switch(&current_id);
            if manager.current_id() != current_id {
                return Err(format!(
                    "inserted session should be current before deletion: expected {current_id}, got {}",
                    manager.current_id()
                ));
            }
            if manager.get(&non_current_id).is_none() {
                return Err("non-current session missing before deletion".into());
            }
            manager.delete_non_current(&non_current_id)?;
        }

        let mgr = state.manager.read().await;
        let manager = match mgr.as_ref() {
            Some(manager) => manager,
            None => return Err("manager should remain after non-current deletion".into()),
        };
        if manager.current_id() != current_id {
            return Err(format!(
                "current session changed after non-current deletion: expected {current_id}, got {}",
                manager.current_id()
            ));
        }
        if manager.get(&non_current_id).is_some() {
            return Err("non-current session remained after deletion".into());
        }
        if manager.metadata(&non_current_id).is_some() {
            return Err("non-current metadata remained after deletion".into());
        }
        if manager.get(&current_id).is_none() {
            return Err("current session missing after non-current deletion".into());
        }

        Ok(())
    }

    #[tokio::test]
    async fn persist_session_writes_to_store() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        state.ensure_manager().await?;

        let session_id = {
            let mgr = state.manager.read().await;
            mgr.as_ref()
                .ok_or("No active session")?
                .current_id()
                .to_string()
        };

        let entry = SessionStoreEntry {
            head: SessionHeadDTO {
                version: SessionSchemaVersion::V1,
                id: session_id.clone(),
                parent_id: None,
                state: FlattenedAgentState::Idle,
                state_stack: Vec::new(),
                extensions: BTreeMap::new(),
                persisted_msg_count: 0,
                compaction_index: 0,
                checksum: None,
            },
            messages: vec![ChatMessage::User {
                content: "hello persistence".into(),
            }],
        };

        {
            let factory = state.factory.read().await;
            let mut store = factory.store.write().await;
            store.insert(session_id.clone(), entry);
        }

        persist_session(&state).await?;

        let factory = state.factory.read().await.clone();
        let head = factory
            .redb
            .load_session(&session_id)
            .await
            .map_err(|e| format!("load session failed: {e}"))?
            .ok_or("session head not found after persist")?;
        assert_eq!(head.id, session_id);

        let messages = factory
            .redb
            .load_messages_for_session(&session_id)
            .await
            .map_err(|e| format!("load messages failed: {e}"))?;
        assert_eq!(messages.len(), 1, "expected one persisted message");
        assert!(messages.iter().any(|(_, m)| matches!(
            m,
            ChatMessage::User { content } if content == "hello persistence"
        )));

        Ok(())
    }
}
