//! Desktop session command families.
//!
//! The module keeps the frontend IPC names stable while separating chat
//! streaming, slash-command parsing, session lifecycle, and attachment I/O.
//!
//! Refs: I-Shell-Runtime-OnlyIO

mod attachments;
mod chat;
mod lifecycle;
mod listing;
mod loading;
mod slash;

pub use chat::{ChatMessagePayload, ChatRole};
pub use listing::{SessionInfo, SessionSort};
use tauri::{AppHandle, State};

use crate::state::DesktopState;

/// Sends a chat message or local slash command through the active desktop session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, DesktopState>,
    content: String,
) -> Result<(), String> {
    chat::send_message(app, state, content).await
}

/// Returns chat history for the active desktop session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn get_messages(
    state: State<'_, DesktopState>,
) -> Result<Vec<ChatMessagePayload>, String> {
    chat::get_messages(state).await
}

/// Clears the active desktop session history.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn clear_messages(state: State<'_, DesktopState>) -> Result<(), String> {
    chat::clear_messages(state).await
}

/// Lists in-memory desktop sessions.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, DesktopState>,
    sort: Option<SessionSort>,
) -> Result<Vec<SessionInfo>, String> {
    listing::list_sessions(state, sort).await
}

/// Switches the active desktop session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn switch_session(
    app: AppHandle,
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    lifecycle::switch_session(app, state, id).await
}

/// Deletes a non-active desktop session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn delete_session(
    app: AppHandle,
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    lifecycle::delete_session(app, state, id).await
}

/// Creates and activates a new desktop session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn new_session(app: AppHandle, state: State<'_, DesktopState>) -> Result<String, String> {
    lifecycle::new_session(app, state).await
}

/// Attaches a filesystem reference to the active chat.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn attach_reference(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<(), String> {
    attachments::attach_reference(app, state, path).await
}

/// Sends an image file as a chat message.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Cancel safety
/// This wrapper holds no locks across await points; cancellation drops the delegated command future.
#[tauri::command]
pub async fn send_image(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<String, String> {
    attachments::send_image(app, state, path).await
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::commands::session::chat::{ChatMessagePayload, ChatRole, get_messages_impl};
    use crate::state::DesktopState;

    pub(crate) fn test_state() -> Result<(DesktopState, tempfile::TempDir), String> {
        let temp_dir =
            tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {e}"))?;
        let path = temp_dir.path().join("test.redb");
        let state = DesktopState::new_with_path(&path)?;
        Ok((state, temp_dir))
    }

    pub(crate) async fn wait_for_system_message(
        state: &DesktopState,
    ) -> Result<Vec<ChatMessagePayload>, String> {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let msgs = get_messages_impl(state).await?;
                if msgs.iter().any(|m| matches!(m.role, ChatRole::System)) {
                    return Ok::<_, String>(msgs);
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| "system prompt should appear within 2s".to_string())?
    }
}
