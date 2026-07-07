//! Session listing and sort projection commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::{DesktopState, SessionMetadata};

pub(super) fn session_lines(manager: &crate::state::SessionManager) -> Vec<String> {
    let mut lines = vec![format!("Current session: {}", manager.current_id())];
    lines.push("Sessions:".into());
    for id in manager.list() {
        let marker = if id == manager.current_id() {
            " → "
        } else {
            "   "
        };
        lines.push(format!("{}{}", marker, id));
    }
    lines
}

/// Session info returned to the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated session info. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: String,
    /// Whether this is the currently active session.
    pub active: bool,
    /// Creation timestamp in seconds since the UNIX epoch.
    pub created_at: u64,
    /// Workspace / working directory associated with the session.
    pub workspace: String,
}

/// How sessions should be sorted when returned to the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) Copy enum.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SessionSort {
    /// Most recently created first.
    #[default]
    Date,
    /// Grouped by workspace, then by date.
    Workspace,
    /// Alphabetical by session id.
    Name,
}

/// Returns the list of all sessions.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S log S) where S is the number of active sessions. Performs list sorting.
///
/// # Panic / Safety
/// Never panics. Returns Err if manager is uninitialized.
pub async fn list_sessions(
    state: State<'_, DesktopState>,
    sort: Option<SessionSort>,
) -> Result<Vec<SessionInfo>, String> {
    state.ensure_manager().await?;
    let mgr = state.manager.read().await;
    let manager = mgr.as_ref().ok_or("No active session")?;
    let current = manager.current_id().to_string();
    let sort = match sort {
        Some(s) => s,
        None => SessionSort::Date,
    };
    let mut sessions: Vec<SessionInfo> = manager
        .list()
        .into_iter()
        .map(|id| {
            let meta = match manager.metadata(id) {
                Some(meta) => meta,
                None => SessionMetadata {
                    id: id.clone(),
                    created_at: 0,
                    workspace: String::new(),
                },
            };
            SessionInfo {
                id: id.clone(),
                active: id == &current,
                created_at: meta.created_at,
                workspace: meta.workspace.clone(),
            }
        })
        .collect();

    sessions.sort_by(|a, b| match sort {
        SessionSort::Date => b.created_at.cmp(&a.created_at),
        SessionSort::Name => a.id.cmp(&b.id),
        SessionSort::Workspace => a
            .workspace
            .cmp(&b.workspace)
            .then_with(|| b.created_at.cmp(&a.created_at)),
    });
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::session::test_support::test_state;

    #[tokio::test]
    async fn session_lines_reflects_current_session() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        state.ensure_manager().await?;
        let mgr = state.manager.read().await;
        let manager = mgr.as_ref().ok_or("No active session")?;
        let lines = session_lines(manager);
        assert!(
            lines.iter().any(|line| line.contains("Current session:")),
            "expected current session line"
        );
        assert!(
            lines.iter().any(|line| line.contains("Sessions:")),
            "expected sessions line"
        );
        Ok(())
    }
}
