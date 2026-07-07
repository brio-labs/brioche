//! Session management, chat messaging, and attachment commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_core::{ChatMessage, EngineInput};
use brioche_shell_runtime::util::system_time_secs;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::{DesktopState, SessionMetadata, persist_session};

/// Role of a chat message participant.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    /// System-level instructions.
    #[default]
    System,
    /// Human user input.
    User,
    /// LLM-generated response.
    Assistant,
    /// Tool invocation request.
    ToolRequest,
    /// Tool execution result.
    ToolResult,
}

/// Payload emitted to the frontend for chat messages.
///
/// The frontend expects `{role, content}` so we flatten the
/// `ChatMessage` enum into this shape before emitting. Optional tool
/// fields are populated for tool request/result messages so the UI can
/// render structured tool cards.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default, Serialize)]
pub struct ChatMessagePayload {
    /// Role of the message sender.
    pub role: ChatRole,
    /// Message content.
    pub content: String,
    /// Tool call identifier, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    /// Tool name, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Tool arguments JSON, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_arguments: Option<String>,
    /// Tool execution output, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<String>,
}

impl From<&ChatMessage> for ChatMessagePayload {
    fn from(msg: &ChatMessage) -> Self {
        let mut base = Self::default();
        match msg {
            ChatMessage::System { content } => {
                base.role = ChatRole::System;
                base.content = content.clone();
            }
            ChatMessage::User { content } => {
                base.role = ChatRole::User;
                base.content = content.clone();
            }
            ChatMessage::Assistant { content, .. } => {
                base.role = ChatRole::Assistant;
                base.content = content.clone();
            }
            ChatMessage::ToolRequest {
                id,
                name,
                arguments,
            } => {
                base.role = ChatRole::ToolRequest;
                base.content = format!("Tool {} ({}): {}", name, id, arguments);
                base.tool_id = Some(id.clone());
                base.tool_name = Some(name.clone());
                base.tool_arguments = Some(arguments.clone());
            }
            ChatMessage::ToolResult { id, content } => {
                base.role = ChatRole::ToolResult;
                base.content = format!("Tool result {}: {}", id, content);
                base.tool_id = Some(id.clone());
                base.tool_output = Some(content.clone());
            }
            _ => {}
        }
        base
    }
}

impl From<ChatMessage> for ChatMessagePayload {
    fn from(msg: ChatMessage) -> Self {
        Self::from(&msg)
    }
}
fn emit_system(app: &AppHandle, content: impl Into<String>) {
    let _ = app.emit(
        "chat-message",
        ChatMessagePayload::from(ChatMessage::System {
            content: content.into(),
        }),
    );
}
fn session_lines(manager: &crate::state::SessionManager) -> Vec<String> {
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

/// Sends a message to the current shell.
///
/// If the message starts with `/`, it is treated as a slash command
/// and executed directly without going to the LLM.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(M) where M is the message size. Initiates async IPC/network orchestration.
///
/// # Panic / Safety
/// Never panics. Returns Err on network or IPC channel failures.
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, DesktopState>,
    content: String,
) -> Result<(), String> {
    send_message_impl(app, state.inner(), content).await
}

async fn send_message_impl(
    app: AppHandle,
    state: &DesktopState,
    content: String,
) -> Result<(), String> {
    state.ensure_manager().await?;

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    // Slash commands are handled locally.
    if let Some(cmd) = trimmed.strip_prefix('/') {
        return handle_slash_command(&app, state, cmd, trimmed).await;
    }

    // Normal message → current session
    let mut mgr = state.manager.write().await;
    let manager = mgr.as_mut().ok_or("No active session")?;
    let shell = manager.current_shell().ok_or("No active session")?.clone();
    let llm = manager.current_llm().ok_or("No active session")?.clone();
    let current_id = manager.current_id().to_string();
    let rx = manager.take_llm_rx();
    drop(mgr);

    // Push to LLM history
    llm.push_message(ChatMessage::User {
        content: trimmed.to_string(),
    })
    .await;

    shell
        .send_input(EngineInput::UserMessage(trimmed.to_string()))
        .await
        .map_err(|e| format!("Send error: {e}"))?;

    // Stream the assistant response to the frontend and auto-save when done.
    if let Some(rx) = rx {
        let app_clone = app.clone();
        forward_llm_chunks(app_clone, rx).await;
        persist_session(state).await?;
    }

    let _ = app.emit(
        "chat-message",
        ChatMessagePayload::from(ChatMessage::System {
            content: format!("Sent to {}: {}", current_id, trimmed),
        }),
    );

    Ok(())
}

/// Forwards LLM chunks from the broadcast receiver to the frontend.
///
/// Each chunk is emitted as a `chat-message` event with role `assistant`.
/// The frontend accumulates these chunks into a streaming response.
///
/// Refs: I-Shell-Runtime-OnlyIO
async fn forward_llm_chunks(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<brioche_provider_openai::LlmChunk>,
) {
    while let Ok(chunk) = rx.recv().await {
        let payload = match chunk {
            brioche_shell_runtime::LlmChunk::Text(text) => ChatMessagePayload {
                role: ChatRole::Assistant,
                content: text,
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::Reasoning(text) => ChatMessagePayload {
                role: ChatRole::Assistant,
                content: text,
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::ToolCallStart { id, name } => ChatMessagePayload {
                role: ChatRole::ToolRequest,
                content: format!("Tool call: {}", name),
                tool_id: Some(id),
                tool_name: Some(name),
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::ToolArgument { fragment, .. } => ChatMessagePayload {
                role: ChatRole::ToolResult,
                content: fragment,
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::ToolCallDone { .. } => ChatMessagePayload {
                role: ChatRole::ToolResult,
                content: String::new(),
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::ToolResult { name, output } => ChatMessagePayload {
                role: ChatRole::ToolResult,
                content: format!("{}: {}", name, output),
                tool_name: Some(name.clone()),
                tool_output: Some(output),
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::Done => continue,
            brioche_shell_runtime::LlmChunk::Error(err) => ChatMessagePayload {
                role: ChatRole::System,
                content: err,
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::Warning(w) => ChatMessagePayload {
                role: ChatRole::System,
                content: w,
                ..ChatMessagePayload::default()
            },
            brioche_shell_runtime::LlmChunk::Status(_) => continue,
        };
        let _ = app.emit("chat-message", payload);
    }
}

/// Returns the current session's chat history.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(H) where H is the size of the conversation history. Reads from memory.
///
/// # Panic / Safety
/// Never panics. Returns Err if no session is active.
#[tauri::command]
pub async fn get_messages(
    state: State<'_, DesktopState>,
) -> Result<Vec<ChatMessagePayload>, String> {
    get_messages_impl(state.inner()).await
}

async fn get_messages_impl(state: &DesktopState) -> Result<Vec<ChatMessagePayload>, String> {
    state.ensure_manager().await?;
    let mgr = state.manager.read().await;
    let manager = mgr.as_ref().ok_or("No active session")?;
    let entry = manager
        .get(manager.current_id())
        .ok_or("No active session")?;
    let history = entry.history.read().await;
    Ok(history.iter().map(ChatMessagePayload::from).collect())
}

/// Clears the current session history and resets the shell.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) in-memory session replacement, triggers shell rebuild.
///
/// # Panic / Safety
/// Never panics. Returns Err if session manager is unavailable or shell rebuild fails.
#[tauri::command]
pub async fn clear_messages(state: State<'_, DesktopState>) -> Result<(), String> {
    rebuild_current_session(state.inner()).await?;
    Ok(())
}
async fn rebuild_current_session(state: &DesktopState) -> Result<String, String> {
    state.ensure_manager().await?;
    let mut mgr = state.manager.write().await;
    let manager = mgr.as_mut().ok_or("No active session")?;
    let current_id = manager.current_id().to_string();
    let factory = state.factory.read().await.clone();
    let handle = crate::commands::shell::build_shell(&current_id, &factory)
        .await
        .map_err(|e| e.to_string())?;
    manager.insert(
        current_id.clone(),
        handle.shell,
        handle.llm,
        handle.history,
        handle.llm_rx,
    );
    if let Some(current_meta) = manager.metadata(&current_id) {
        manager.insert_metadata(current_meta)?;
    }
    Ok(current_id)
}

/// Processes a slash command.
///
/// Supported commands:
/// - `/help` — show help text
/// - `/quit` — exit the app (emitted as event)
/// - `/clear` — clear history
/// - `/session` — show current session info
/// - `/session new` — create a new session
/// - `/session list` — list all sessions
/// - `/session load <id>` — load a persisted session
///
/// Refs: I-Shell-Runtime-OnlyIO
async fn handle_slash_command(
    app: &AppHandle,
    state: &DesktopState,
    cmd: &str,
    full_line: &str,
) -> Result<(), String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    match parts.first().copied() {
        Some("help") | Some("h") => {
            emit_system(app, print_help());
            Ok(())
        }
        Some("quit") | Some("q") => {
            let _ = app.emit("app-exit", ());
            Ok(())
        }
        Some("clear") | Some("c") => {
            rebuild_current_session(state).await?;
            emit_system(app, "History cleared.");
            Ok(())
        }
        Some("session") => handle_session_subcommand(app, state, &parts[1..]).await,
        _ => {
            emit_system(app, format!("Unknown command: {full_line}"));
            Ok(())
        }
    }
}

async fn handle_session_subcommand(
    app: &AppHandle,
    state: &DesktopState,
    args: &[&str],
) -> Result<(), String> {
    let Some(command) = args.first().copied() else {
        emit_system(app, "/session requires a sub-command");
        return Ok(());
    };

    match command {
        "new" => {
            let new_id = new_session_impl(state).await?;
            emit_system(app, format!("New session: {new_id}"));
        }
        "list" | "" => {
            state.ensure_manager().await?;
            let mgr = state.manager.read().await;
            if let Some(manager) = mgr.as_ref() {
                emit_system(app, session_lines(manager).join("\n"));
            }
        }
        "load" => {
            let Some(id) = args.get(1).copied() else {
                emit_system(app, "/session load requires a session id");
                return Ok(());
            };
            load_session(app, state, id).await?;
        }
        other => emit_system(app, format!("Unknown /session {} command", other)),
    }
    Ok(())
}

/// Help text for slash commands.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn print_help() -> String {
    [
        "Commands:",
        "  <text>               Send a message to the LLM",
        "  /help                Show this help",
        "  /quit                Exit the app",
        "  /clear               Clear conversation history",
        "  /session             Show current session",
        "  /session new         Create a new session",
        "  /session list        List sessions",
        "  /session load <id>   Load a persisted session",
        "",
        "Environment variables:",
        "  BRIOCHE_API_KEY      API key",
        "  BRIOCHE_MODEL        LLM model (default: gpt-4o-mini)",
        "  BRIOCHE_BASE_URL     API endpoint",
    ]
    .join("\n")
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
#[tauri::command]
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

/// Switches to an existing session.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S + M) where S is the number of sessions and M is memory provider initialization.
///
/// # Panic / Safety
/// Never panics. Returns Err if the session is not found or memory provider initialization fails.
#[tauri::command]
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
#[tauri::command]
pub async fn delete_session(
    app: AppHandle,
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    state.ensure_manager().await?;
    let mut mgr = state.manager.write().await;
    let manager = mgr.as_mut().ok_or("No active session")?;
    if manager.current_id() == id {
        return Err("Cannot delete the active session".into());
    }
    manager.sessions.remove(&id);
    manager.remove_metadata(&id)?;
    drop(mgr);
    let _ = app.emit("sessions-updated", ());
    Ok(())
}

async fn new_session_impl(state: &DesktopState) -> Result<String, String> {
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
#[tauri::command]
pub async fn new_session(app: AppHandle, state: State<'_, DesktopState>) -> Result<String, String> {
    let id = new_session_impl(state.inner()).await?;
    let _ = app.emit("session-changed", id.clone());
    let _ = app.emit("sessions-updated", ());
    Ok(id)
}

async fn load_session(app: &AppHandle, state: &DesktopState, id: &str) -> Result<(), String> {
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
async fn load_session_impl(state: &DesktopState, id: &str) -> Result<Vec<ChatMessage>, String> {
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

/// Attaches a file or folder reference to the current conversation.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) plus the cost of reading filesystem metadata. Sends one user message.
///
/// # Panic / Safety
/// Never panics. Returns Err if the path cannot be read or no session is active.
#[tauri::command]
pub async fn attach_reference(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<(), String> {
    let content = attach_reference_impl(state.inner(), path).await?;
    emit_system(&app, content);
    Ok(())
}

/// Implementation of [`attach_reference`] that does not need a Tauri
/// [`AppHandle`], so it can be exercised from library tests.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) plus the cost of reading filesystem metadata. Sends one user message.
///
/// # Panic / Safety
/// Never panics. Returns Err if the path cannot be read or no session is active.
async fn attach_reference_impl(state: &DesktopState, path: String) -> Result<String, String> {
    state.ensure_manager().await?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("Failed to read reference: {e}"))?;
    let kind = if metadata.is_dir() { "folder" } else { "file" };
    let content = format!("User attached {kind}: {path}");
    {
        let mgr = state.manager.read().await;
        let manager = mgr.as_ref().ok_or("No active session")?;
        let entry = manager
            .get(manager.current_id())
            .ok_or("No active session")?;
        entry
            .llm
            .push_message(ChatMessage::User {
                content: content.clone(),
            })
            .await;
    }
    Ok(content)
}

/// Sends an image attachment for multimodal models.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(B) where B is the image file size. Encodes the image as base64.
///
/// # Panic / Safety
/// Never panics. Returns Err if the image cannot be read or no session is active.
#[tauri::command]
pub async fn send_image(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<String, String> {
    let (content, data_url) = send_image_impl(state.inner(), path).await?;
    emit_system(&app, content);
    Ok(data_url)
}

/// Implementation of [`send_image`] that does not need a Tauri
/// [`AppHandle`], so it can be exercised from library tests.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(B) where B is the image file size. Encodes the image as base64.
///
/// # Panic / Safety
/// Never panics. Returns Err if the image cannot be read or no session is active.
async fn send_image_impl(state: &DesktopState, path: String) -> Result<(String, String), String> {
    state.ensure_manager().await?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("Failed to read image: {e}"))?;
    let mime = match std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    let b64 = base64_simd::STANDARD.encode_to_string(&bytes);
    let data_url = format!("data:{mime};base64,{b64}");
    let content = format!("User sent an image: {path}\n\n![image]({data_url})");
    {
        let mgr = state.manager.read().await;
        let manager = mgr.as_ref().ok_or("No active session")?;
        let entry = manager
            .get(manager.current_id())
            .ok_or("No active session")?;
        entry
            .llm
            .push_message(ChatMessage::User {
                content: content.clone(),
            })
            .await;
    }
    Ok((content, data_url))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use brioche_shell_persistence::{
        FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion, SessionStoreEntry,
    };

    use super::*;
    use crate::state::DesktopState;

    fn test_state() -> Result<(DesktopState, tempfile::TempDir), String> {
        let temp_dir =
            tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {e}"))?;
        let path = temp_dir.path().join("test.redb");
        let state = DesktopState::new_with_path(&path)?;
        Ok((state, temp_dir))
    }

    async fn wait_for_system_message(
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

    #[tokio::test]
    async fn ensure_manager_lazily_initializes() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        assert!(state.manager.read().await.is_none());
        state.ensure_manager().await?;
        assert!(state.manager.read().await.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn get_messages_has_system_prompt_on_fresh_session() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        let messages = wait_for_system_message(&state).await?;
        assert!(
            messages.iter().any(|m| matches!(m.role, ChatRole::System)),
            "expected at least one system message in fresh session history"
        );
        Ok(())
    }

    #[tokio::test]
    async fn clear_messages_resets_session() -> Result<(), String> {
        let (state, _temp) = test_state()?;
        rebuild_current_session(&state).await?;
        let messages = wait_for_system_message(&state).await?;
        assert!(
            messages.iter().any(|m| matches!(m.role, ChatRole::System)),
            "expected at least one system message after clear"
        );
        Ok(())
    }

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

    #[tokio::test]
    async fn attach_reference_impl_attaches_existing_file() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let file_path = temp.path().join("reference.txt");
        tokio::fs::write(&file_path, "hello")
            .await
            .map_err(|e| format!("Failed to write test file: {e}"))?;
        let content =
            attach_reference_impl(&state, file_path.to_string_lossy().to_string()).await?;
        assert!(
            content.contains("User attached file"),
            "expected file attachment"
        );
        assert!(
            content.contains(file_path.to_string_lossy().as_ref()),
            "expected path in attachment"
        );
        Ok(())
    }

    #[tokio::test]
    async fn attach_reference_impl_errors_for_missing_file() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let file_path = temp.path().join("missing.txt");
        let result = attach_reference_impl(&state, file_path.to_string_lossy().to_string()).await;
        let err = match result {
            Err(e) => e,
            Ok(_) => return Err("expected error for missing file".into()),
        };
        assert!(
            err.contains("Failed to read reference"),
            "expected read reference error"
        );
        Ok(())
    }

    #[tokio::test]
    async fn send_image_impl_encodes_existing_image() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let image_path = temp.path().join("image.png");
        tokio::fs::write(&image_path, b"\x89PNG\r\n\x1a\n")
            .await
            .map_err(|e| format!("Failed to write test image: {e}"))?;
        let (content, data_url) =
            send_image_impl(&state, image_path.to_string_lossy().to_string()).await?;
        assert!(
            content.contains("User sent an image"),
            "expected image attachment"
        );
        assert!(
            data_url.starts_with("data:image/png;base64,"),
            "expected png data url"
        );
        Ok(())
    }

    #[tokio::test]
    async fn send_image_impl_errors_for_missing_image() -> Result<(), String> {
        let (state, temp) = test_state()?;
        state.ensure_manager().await?;
        let image_path = temp.path().join("missing.png");
        let result = send_image_impl(&state, image_path.to_string_lossy().to_string()).await;
        let err = match result {
            Err(e) => e,
            Ok(_) => return Err("expected error for missing image".into()),
        };
        assert!(
            err.contains("Failed to read image"),
            "expected read image error"
        );
        Ok(())
    }

    #[test]
    fn print_help_contains_commands() {
        let help = print_help();
        assert!(help.contains("/help"), "expected /help in help text");
        assert!(help.contains("/session"), "expected /session in help text");
    }

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
