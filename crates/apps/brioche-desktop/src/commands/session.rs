//! Session management, chat messaging, and attachment commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_core::{ChatMessage, EngineInput};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::{DesktopState, SessionMetadata};

/// Role of a chat message participant.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) Copy enum.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    /// System-level instructions.
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
///
/// # Complexity
/// O(1) creation. Contains heap-allocated Strings.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
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

impl Default for ChatMessagePayload {
    fn default() -> Self {
        Self {
            role: ChatRole::System,
            content: String::new(),
            tool_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_output: None,
        }
    }
}

impl From<&ChatMessage> for ChatMessagePayload {
    fn from(msg: &ChatMessage) -> Self {
        match msg {
            ChatMessage::System { content } => Self {
                role: ChatRole::System,
                content: content.clone(),
                ..Self::default()
            },
            ChatMessage::User { content } => Self {
                role: ChatRole::User,
                content: content.clone(),
                ..Self::default()
            },
            ChatMessage::Assistant { content, .. } => Self {
                role: ChatRole::Assistant,
                content: content.clone(),
                ..Self::default()
            },
            ChatMessage::ToolRequest {
                id,
                name,
                arguments,
            } => Self {
                role: ChatRole::ToolRequest,
                content: format!("Tool {} ({}): {}", name, id, arguments),
                tool_id: Some(id.clone()),
                tool_name: Some(name.clone()),
                tool_arguments: Some(arguments.clone()),
                ..Self::default()
            },
            ChatMessage::ToolResult { id, content } => Self {
                role: ChatRole::ToolResult,
                content: format!("Tool result {}: {}", id, content),
                tool_id: Some(id.clone()),
                tool_output: Some(content.clone()),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}

impl From<ChatMessage> for ChatMessagePayload {
    fn from(msg: ChatMessage) -> Self {
        Self::from(&msg)
    }
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

    // Take the LLM receiver if available and spawn the forwarder.
    if let Some(rx) = manager.take_llm_rx() {
        let app_clone = app.clone();
        tokio::spawn(forward_llm_chunks(app_clone, rx));
    }
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
/// Never panics. Returns Err if manager fails to initialize.
#[tauri::command]
pub async fn clear_messages(state: State<'_, DesktopState>) -> Result<(), String> {
    clear_messages_impl(state.inner()).await
}

async fn clear_messages_impl(state: &DesktopState) -> Result<(), String> {
    state.ensure_manager().await?;
    let mut mgr = state.manager.write().await;
    let manager = mgr.as_mut().ok_or("No active session")?;
    let current_id = manager.current_id().to_string();
    let _config = state.config.read().await.clone();
    let factory = state.factory.read().await.clone();
    let handle = crate::commands::shell::build_shell(&current_id, &factory);
    manager.insert(
        current_id,
        handle.shell,
        handle.llm,
        handle.history,
        handle.llm_rx,
    );
    Ok(())
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
            let _ = app.emit(
                "chat-message",
                ChatMessagePayload::from(ChatMessage::System {
                    content: print_help(),
                }),
            );
            Ok(())
        }
        Some("quit") | Some("q") => {
            let _ = app.emit("app-exit", ());
            Ok(())
        }
        Some("clear") | Some("c") => {
            let mut mgr = state.manager.write().await;
            let manager = mgr.as_mut().ok_or("No active session")?;
            let current_id = manager.current_id().to_string();
            let _config = state.config.read().await.clone();
            let factory = state.factory.read().await.clone();
            let handle = crate::commands::shell::build_shell(&current_id, &factory);
            manager.insert(
                current_id,
                handle.shell,
                handle.llm,
                handle.history,
                handle.llm_rx,
            );
            let _ = app.emit(
                "chat-message",
                ChatMessagePayload::from(ChatMessage::System {
                    content: "History cleared.".into(),
                }),
            );
            Ok(())
        }
        Some("session") if parts.len() == 1 => {
            let mgr = state.manager.read().await;
            let manager = mgr.as_ref().ok_or("No active session")?;
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
            let _ = app.emit(
                "chat-message",
                ChatMessagePayload::from(ChatMessage::System {
                    content: lines.join("\n"),
                }),
            );
            Ok(())
        }
        Some("session") if parts.len() >= 2 => {
            handle_session_subcommand(app, state, &parts[1..]).await;
            Ok(())
        }
        _ => {
            let _ = app.emit(
                "chat-message",
                ChatMessagePayload::from(ChatMessage::System {
                    content: format!("Unknown command: {full_line}"),
                }),
            );
            Ok(())
        }
    }
}

async fn handle_session_subcommand(app: &AppHandle, state: &DesktopState, args: &[&str]) {
    let Some(command) = args.first().copied() else {
        let _ = app.emit(
            "chat-message",
            ChatMessagePayload::from(ChatMessage::System {
                content: "/session requires a sub-command".into(),
            }),
        );
        return;
    };

    match command {
        "new" => {
            let new_id = format!(
                "session-{}",
                match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                    Ok(d) => d.as_secs(),
                    Err(_) => 0,
                }
            );
            let factory = state.factory.read().await.clone();
            let workspace = factory.settings.working_dir();
            let handle = crate::commands::shell::build_shell(&new_id, &factory);
            {
                let mut mgr = state.manager.write().await;
                if let Some(manager) = mgr.as_mut() {
                    manager.insert(
                        new_id.clone(),
                        handle.shell,
                        handle.llm,
                        handle.history,
                        handle.llm_rx,
                    );
                    manager
                        .metadata_store
                        .insert(SessionMetadata::new(&new_id, &workspace));
                    let _ = manager.metadata_store.save();
                    manager.switch(&new_id);
                }
            }
            let _ = app.emit(
                "chat-message",
                ChatMessagePayload::from(ChatMessage::System {
                    content: format!("New session: {}", new_id),
                }),
            );
        }
        "list" => {
            let mgr = state.manager.read().await;
            if let Some(manager) = mgr.as_ref() {
                let mut lines = vec!["Sessions:".into()];
                for id in manager.list() {
                    let marker = if id == manager.current_id() {
                        " → "
                    } else {
                        "   "
                    };
                    lines.push(format!("{}{}", marker, id));
                }
                let _ = app.emit(
                    "chat-message",
                    ChatMessagePayload::from(ChatMessage::System {
                        content: lines.join("\n"),
                    }),
                );
            }
        }
        "load" => {
            let Some(id) = args.get(1).copied() else {
                let _ = app.emit(
                    "chat-message",
                    ChatMessagePayload::from(ChatMessage::System {
                        content: "/session load requires a session id".into(),
                    }),
                );
                return;
            };
            let factory = state.factory.read().await.clone();
            let workspace = factory.settings.working_dir();
            let _head = match factory.redb.load_session(id).await {
                Ok(Some(h)) => h,
                Ok(None) => {
                    let _ = app.emit(
                        "chat-message",
                        ChatMessagePayload::from(ChatMessage::System {
                            content: format!("Session '{}' not found.", id),
                        }),
                    );
                    return;
                }
                Err(err) => {
                    let _ = app.emit(
                        "chat-message",
                        ChatMessagePayload::from(ChatMessage::System {
                            content: format!("Load error: {err}"),
                        }),
                    );
                    return;
                }
            };
            let messages: Vec<ChatMessage> = match factory.redb.load_messages_for_session(id).await
            {
                Ok(msgs) => msgs.into_iter().map(|(_, m)| m).collect(),
                Err(err) => {
                    let _ = app.emit(
                        "chat-message",
                        ChatMessagePayload::from(ChatMessage::System {
                            content: format!("Load messages error: {err}"),
                        }),
                    );
                    return;
                }
            };
            let _config = state.config.read().await.clone();
            let handle = crate::commands::shell::build_shell(id, &factory);
            {
                let mut mgr = state.manager.write().await;
                if let Some(manager) = mgr.as_mut() {
                    manager.insert(
                        id.to_string(),
                        handle.shell,
                        handle.llm,
                        handle.history,
                        handle.llm_rx,
                    );
                    if manager.metadata_store.get(id).created_at == 0 {
                        manager
                            .metadata_store
                            .insert(SessionMetadata::new(id, &workspace));
                        let _ = manager.metadata_store.save();
                    }
                    manager.switch(id);
                }
            }
            let _ = app.emit(
                "chat-message",
                ChatMessagePayload::from(ChatMessage::System {
                    content: format!("Session '{}' loaded ({} messages).", id, messages.len()),
                }),
            );
        }
        other => {
            let _ = app.emit(
                "chat-message",
                ChatMessagePayload::from(ChatMessage::System {
                    content: format!("Unknown /session {} command", other),
                }),
            );
        }
    }
}

/// Help text for slash commands.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn print_help() -> String {
    let lines = vec![
        "Commands:".into(),
        "  <text>               Send a message to the LLM".into(),
        "  /help                Show this help".into(),
        "  /quit                Exit the app".into(),
        "  /clear               Clear conversation history".into(),
        "  /session             Show current session".into(),
        "  /session new         Create a new session".into(),
        "  /session list        List sessions".into(),
        "  /session load <id>   Load a persisted session".into(),
        String::new(),
        "Environment variables:".into(),
        "  BRIOCHE_API_KEY      API key".into(),
        "  BRIOCHE_MODEL        LLM model (default: gpt-4o-mini)".into(),
        "  BRIOCHE_BASE_URL     API endpoint".into(),
    ];
    lines.join("\n")
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
            let meta = manager.metadata_store.get(id);
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
/// O(log S) switch where S is the number of active sessions. Emits Tauri events.
///
/// # Panic / Safety
/// Never panics. Returns Err if the session ID is not found.
#[tauri::command]
pub async fn switch_session(
    app: AppHandle,
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    state.ensure_manager().await?;
    let mut mgr = state.manager.write().await;
    let manager = mgr.as_mut().ok_or("No active session")?;
    if !manager.list().iter().any(|sid| sid == &&id) {
        return Err(format!("Session '{}' not found", id));
    }
    manager.switch(&id);
    drop(mgr);
    // Emit a system message so the frontend knows we switched
    let _ = app.emit(
        "chat-message",
        ChatMessagePayload::from(ChatMessage::System {
            content: format!("Switched to session: {}", id),
        }),
    );
    // Emit the session-changed event so the frontend can refresh
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
/// Never panics. Returns Err if attempting to delete the active session.
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
    manager.metadata_store.remove(&id);
    let _ = manager.metadata_store.save();
    drop(mgr);
    let _ = app.emit("sessions-updated", ());
    Ok(())
}

/// Creates a new session and switches to it.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) in-memory creation and storage writing. Rebuilds the shell in background.
///
/// # Panic / Safety
/// Never panics. Returns Err if manager is uninitialized.
#[tauri::command]
pub async fn new_session(app: AppHandle, state: State<'_, DesktopState>) -> Result<String, String> {
    state.ensure_manager().await?;
    let new_id = format!(
        "session-{}",
        match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        }
    );
    let factory = state.factory.read().await.clone();
    let workspace = factory.settings.working_dir();
    let handle = crate::commands::shell::build_shell(&new_id, &factory);
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
        manager
            .metadata_store
            .insert(SessionMetadata::new(&new_id, &workspace));
        let _ = manager.metadata_store.save();
        manager.switch(&new_id);
    }
    let _ = app.emit("session-changed", new_id.clone());
    let _ = app.emit("sessions-updated", ());
    Ok(new_id)
}

/// Attaches a file or folder reference to the current conversation.
///
/// The reference is emitted as a system message so the model sees it.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) file metadata read and event emission.
///
/// # Panic / Safety
/// Never panics. Returns Err if file or folder metadata cannot be read.
#[tauri::command]
pub async fn attach_reference(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<(), String> {
    state.ensure_manager().await?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("Failed to read reference: {e}"))?;
    let kind = if metadata.is_dir() { "folder" } else { "file" };
    let content = format!("User attached {kind}: {path}");
    let _ = app.emit(
        "chat-message",
        ChatMessagePayload::from(ChatMessage::System { content }),
    );
    Ok(())
}

/// Sends an image attachment for multimodal models.
///
/// The image bytes are read from disk and encoded as a data URL.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(B) where B is the number of image bytes read and base64-encoded.
///
/// # Panic / Safety
/// Never panics. Returns Err if the image file cannot be read.
#[tauri::command]
pub async fn send_image(
    app: AppHandle,
    state: State<'_, DesktopState>,
    path: String,
) -> Result<String, String> {
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
    let _ = app.emit(
        "chat-message",
        ChatMessagePayload::from(ChatMessage::System { content }),
    );
    Ok(data_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DesktopState;

    /// Verifies that `DesktopState` can be created and `ensure_manager`
    /// initializes the session manager lazily.
    #[tokio::test]
    async fn ensure_manager_lazily_initializes() {
        let init_result = DesktopState::new_with_path("/tmp/brioche-desktop-test-ensure.redb");
        assert!(init_result.is_ok(), "test state should initialize");
        let state = match init_result {
            Ok(s) => s,
            Err(_) => return,
        };
        assert!(state.manager.read().await.is_none());
        assert!(
            state.ensure_manager().await.is_ok(),
            "ensure_manager should succeed"
        );
        assert!(state.manager.read().await.is_some());
    }

    /// Verifies that `get_messages` returns the system prompt for a
    /// fresh session.
    #[tokio::test]
    async fn get_messages_has_system_prompt_on_fresh_session() {
        let init_result = DesktopState::new_with_path("/tmp/brioche-desktop-test-messages.redb");
        assert!(init_result.is_ok(), "test state should initialize");
        let state = match init_result {
            Ok(s) => s,
            Err(_) => return,
        };
        // Retry a few times to account for async system prompt injection.
        let timeout_result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let msgs = match get_messages_impl(&state).await {
                    Ok(m) => m,
                    Err(_) => return Vec::new(),
                };
                if msgs.iter().any(|m| matches!(m.role, ChatRole::System)) {
                    break msgs;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(
            timeout_result.is_ok(),
            "system prompt should appear within 2s"
        );
        let messages = match timeout_result {
            Ok(m) => m,
            Err(_) => return,
        };
        assert!(
            messages.iter().any(|m| matches!(m.role, ChatRole::System)),
            "expected at least one system message in fresh session history"
        );
    }

    /// Verifies that `clear_messages` resets the current session.
    #[tokio::test]
    async fn clear_messages_resets_session() {
        let init_result = DesktopState::new_with_path("/tmp/brioche-desktop-test-clear.redb");
        assert!(init_result.is_ok(), "test state should initialize");
        let state = match init_result {
            Ok(s) => s,
            Err(_) => return,
        };
        assert!(
            clear_messages_impl(&state).await.is_ok(),
            "clear_messages should succeed"
        );
        // Retry a few times to account for async system prompt injection.
        let timeout_result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let msgs = match get_messages_impl(&state).await {
                    Ok(m) => m,
                    Err(_) => return Vec::new(),
                };
                if msgs.iter().any(|m| matches!(m.role, ChatRole::System)) {
                    break msgs;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(
            timeout_result.is_ok(),
            "system prompt should appear within 2s"
        );
        let messages = match timeout_result {
            Ok(m) => m,
            Err(_) => return,
        };
        assert!(
            messages.iter().any(|m| matches!(m.role, ChatRole::System)),
            "expected at least one system message after clear"
        );
    }
}
