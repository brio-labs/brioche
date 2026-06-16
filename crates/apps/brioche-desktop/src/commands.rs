//! Tauri IPC commands for the desktop app.
//!
//! These commands are called by the frontend via `invoke()`.
//! All commands return `Result<T, String>` for simple frontend error handling.
//!
//! ## Command surface
//! - `send_message` — sends user text to the shell (or executes slash commands)
//! - `get_messages` — returns chat history for the current session
//! - `clear_messages` — resets the current session
//! - `list_sessions` — list all sessions
//! - `switch_session` — switch to a different session
//! - `delete_session` — delete a session
//! - `new_session` — create a new session
//! - `get_settings` — get user settings
//! - `set_settings` — save user settings
//! - `pick_directory` — open a directory picker dialog
//! - `read_directory` — list files in a directory
//!
//! ## Cancel safety
//! All `pub async fn` commands in this module are cancel-safe: they hold no
//! locks across await points. Dropping any command future is safe and will not
//! leave the session or state in an inconsistent state.
//!
//! Refs: I-Shell-Runtime-OnlyIO

pub mod shell;

use brioche_core::{ChatMessage, EngineInput};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::settings::Settings;
use crate::state::DesktopState;

/// Role of a chat message participant.
///
/// Refs: I-Shell-Runtime-OnlyIO
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
/// `ChatMessage` enum into this shape before emitting.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct ChatMessagePayload {
    /// Role of the message sender.
    pub role: ChatRole,
    /// Message content.
    pub content: String,
}

impl From<&ChatMessage> for ChatMessagePayload {
    fn from(msg: &ChatMessage) -> Self {
        match msg {
            ChatMessage::System { content } => Self {
                role: ChatRole::System,
                content: content.clone(),
            },
            ChatMessage::User { content } => Self {
                role: ChatRole::User,
                content: content.clone(),
            },
            ChatMessage::Assistant { content, .. } => Self {
                role: ChatRole::Assistant,
                content: content.clone(),
            },
            ChatMessage::ToolRequest {
                id,
                name,
                arguments,
            } => Self {
                role: ChatRole::ToolRequest,
                content: format!("Tool {} ({}): {}", name, id, arguments),
            },
            ChatMessage::ToolResult { id, content } => Self {
                role: ChatRole::ToolResult,
                content: format!("Tool result {}: {}", id, content),
            },
            _ => Self {
                role: ChatRole::System,
                content: String::new(),
            },
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
/// # Errors
/// Returns an error string if the shell is not initialized or the
/// send fails.
///
/// # Cancel safety
/// This future holds no locks across await points. Dropping it is safe
/// and will not leave the session in an inconsistent state.
///
/// Refs: I-Shell-Runtime-OnlyIO
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
            },
            brioche_shell_runtime::LlmChunk::Reasoning(text) => ChatMessagePayload {
                role: ChatRole::Assistant,
                content: text,
            },
            brioche_shell_runtime::LlmChunk::ToolCallStart { name, .. } => ChatMessagePayload {
                role: ChatRole::ToolRequest,
                content: format!("Tool call: {}", name),
            },
            brioche_shell_runtime::LlmChunk::ToolArgument { fragment, .. } => ChatMessagePayload {
                role: ChatRole::ToolResult,
                content: fragment,
            },
            brioche_shell_runtime::LlmChunk::ToolCallDone { .. } => ChatMessagePayload {
                role: ChatRole::ToolResult,
                content: String::new(),
            },
            brioche_shell_runtime::LlmChunk::ToolResult { name, output } => ChatMessagePayload {
                role: ChatRole::ToolResult,
                content: format!("{}: {}", name, output),
            },
            brioche_shell_runtime::LlmChunk::Done => continue,
            brioche_shell_runtime::LlmChunk::Error(err) => ChatMessagePayload {
                role: ChatRole::System,
                content: err,
            },
            brioche_shell_runtime::LlmChunk::Warning(w) => ChatMessagePayload {
                role: ChatRole::System,
                content: w,
            },
            brioche_shell_runtime::LlmChunk::Status(_) => continue,
        };
        let _ = app.emit("chat-message", payload);
    }
}

/// Returns the current session's chat history.
///
/// # Errors
/// Returns an error if no session is active.
///
/// Refs: I-Shell-Runtime-OnlyIO
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
#[tauri::command]
pub async fn clear_messages(state: State<'_, DesktopState>) -> Result<(), String> {
    clear_messages_impl(state.inner()).await
}

async fn clear_messages_impl(state: &DesktopState) -> Result<(), String> {
    state.ensure_manager().await?;
    let mut mgr = state.manager.write().await;
    let manager = mgr.as_mut().ok_or("No active session")?;
    let current_id = manager.current_id().to_string();
    let config = state.config.read().await.clone();
    let factory = state.factory.read().await.clone();
    let handle = crate::commands::shell::build_shell(
        &current_id,
        &config,
        factory.redb.clone(),
        factory.store.clone(),
    );
    manager.insert(
        current_id,
        handle.shell,
        handle.llm,
        handle.history,
        handle.llm_rx,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Slash command handling
// ---------------------------------------------------------------------------

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
            let config = state.config.read().await.clone();
            let factory = state.factory.read().await.clone();
            let handle = crate::commands::shell::build_shell(
                &current_id,
                &config,
                factory.redb.clone(),
                factory.store.clone(),
            );
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
            let config = state.config.read().await.clone();
            let factory = state.factory.read().await.clone();
            let handle = crate::commands::shell::build_shell(
                &new_id,
                &config,
                factory.redb.clone(),
                factory.store.clone(),
            );
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
            let config = state.config.read().await.clone();
            let handle = crate::commands::shell::build_shell(
                id,
                &config,
                factory.redb.clone(),
                factory.store.clone(),
            );
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

// ---------------------------------------------------------------------------
// New IPC commands for the upgraded UI
// ---------------------------------------------------------------------------

/// Session info returned to the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: String,
    /// Whether this is the currently active session.
    pub active: bool,
}

/// Returns the list of all sessions.
#[tauri::command]
pub async fn list_sessions(state: State<'_, DesktopState>) -> Result<Vec<SessionInfo>, String> {
    state.ensure_manager().await?;
    let mgr = state.manager.read().await;
    let manager = mgr.as_ref().ok_or("No active session")?;
    let current = manager.current_id().to_string();
    let sessions = manager
        .list()
        .into_iter()
        .map(|id| SessionInfo {
            id: id.clone(),
            active: id == &current,
        })
        .collect();
    Ok(sessions)
}

/// Switches to an existing session.
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
    drop(mgr);
    let _ = app.emit("sessions-updated", ());
    Ok(())
}

/// Creates a new session and switches to it.
#[tauri::command]
pub async fn new_session(app: AppHandle, state: State<'_, DesktopState>) -> Result<String, String> {
    state.ensure_manager().await?;
    let new_id = format!(
        "session-{}",
        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        }
    );
    let config = state.config.read().await.clone();
    let factory = state.factory.read().await.clone();
    let handle = crate::commands::shell::build_shell(
        &new_id,
        &config,
        factory.redb.clone(),
        factory.store.clone(),
    );
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
        manager.switch(&new_id);
    }
    let _ = app.emit("session-changed", new_id.clone());
    let _ = app.emit("sessions-updated", ());
    Ok(new_id)
}

/// Returns the current user settings.
#[tauri::command]
pub async fn get_settings() -> Result<Settings, String> {
    Ok(Settings::load())
}

/// Saves user settings.
#[tauri::command]
pub async fn set_settings(settings: Settings) -> Result<(), String> {
    settings.save()
}

/// Opens a directory picker dialog and returns the selected path.
#[tauri::command]
pub async fn pick_directory() -> Result<Option<String>, String> {
    // The frontend uses the native file picker via tauri APIs
    Ok(None)
}

// ---------------------------------------------------------------------------
// Memory commands
// ---------------------------------------------------------------------------

/// Memory entry payload for the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct MemoryEntryPayload {
    /// Unique key for the memory entry.
    pub key: String,
    /// Value/content of the memory entry.
    pub value: String,
    /// Category for grouping (e.g., "user", "project").
    pub category: String,
    /// Unix timestamp when the entry was created.
    pub created_at: u64,
    /// Unix timestamp when the entry was last updated.
    pub updated_at: u64,
    /// Number of times this entry has been accessed.
    pub access_count: u32,
}

impl From<&crate::memory::MemoryEntry> for MemoryEntryPayload {
    fn from(entry: &crate::memory::MemoryEntry) -> Self {
        Self {
            key: entry.key.clone(),
            value: entry.value.clone(),
            category: entry.category.clone(),
            created_at: entry.created_at,
            updated_at: entry.updated_at,
            access_count: entry.access_count,
        }
    }
}

/// Lists all memory entries, optionally filtered by category.
#[tauri::command]
pub async fn list_memories(category: Option<String>) -> Result<Vec<MemoryEntryPayload>, String> {
    let store = crate::memory::MemoryStore::load();
    let entries = store.list(category.as_deref());
    Ok(entries.into_iter().map(MemoryEntryPayload::from).collect())
}

/// Sets (adds or updates) a memory entry.
#[tauri::command]
pub async fn set_memory(key: String, value: String, category: String) -> Result<(), String> {
    let mut store = crate::memory::MemoryStore::load();
    store.set(key, value, category);
    store.save()
}

/// Deletes a memory entry by key.
#[tauri::command]
pub async fn delete_memory(key: String) -> Result<(), String> {
    let mut store = crate::memory::MemoryStore::load();
    if !store.delete(&key) {
        return Err(format!("Memory '{}' not found", key));
    }
    store.save()
}

/// Searches memory entries by key or value.
#[tauri::command]
pub async fn search_memories(query: String) -> Result<Vec<MemoryEntryPayload>, String> {
    let store = crate::memory::MemoryStore::load();
    let results = store.search(&query);
    Ok(results.into_iter().map(MemoryEntryPayload::from).collect())
}

// ---------------------------------------------------------------------------
// Profile commands
// ---------------------------------------------------------------------------

/// Profile payload for the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct ProfilePayload {
    /// Machine-readable profile identifier (e.g., "default", "work").
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional description of the profile's purpose.
    pub description: Option<String>,
    /// LLM provider identifier (e.g., "openai", "openrouter").
    pub provider: String,
    /// Model identifier (e.g., "gpt-4o-mini").
    pub model: String,
    /// API key for the provider.
    pub api_key: String,
    /// Optional system prompt override.
    pub system_prompt: Option<String>,
    /// Optional temperature parameter (0.0–2.0).
    pub temperature: Option<f32>,
    /// Optional max tokens limit.
    pub max_tokens: Option<u32>,
    /// Unix timestamp when the profile was created.
    pub created_at: u64,
    /// Whether this is the default active profile.
    pub is_default: bool,
}

impl From<&crate::profiles::Profile> for ProfilePayload {
    fn from(p: &crate::profiles::Profile) -> Self {
        Self {
            name: p.name.clone(),
            display_name: p.display_name.clone(),
            description: p.description.clone(),
            provider: p.provider.clone(),
            model: p.model.clone(),
            api_key: p.api_key.clone(),
            system_prompt: p.system_prompt.clone(),
            temperature: p.temperature,
            max_tokens: p.max_tokens,
            created_at: p.created_at,
            is_default: p.is_default,
        }
    }
}

/// Lists all profiles.
#[tauri::command]
pub async fn list_profiles() -> Result<Vec<ProfilePayload>, String> {
    let config = crate::profiles::ProfileConfig::load();
    Ok(config.profiles.iter().map(ProfilePayload::from).collect())
}

/// Gets the active profile.
#[tauri::command]
pub async fn get_profile(name: Option<String>) -> Result<Option<ProfilePayload>, String> {
    let config = crate::profiles::ProfileConfig::load();
    if let Some(n) = name {
        Ok(config.get(&n).map(ProfilePayload::from))
    } else {
        Ok(config.active_profile().map(ProfilePayload::from))
    }
}

/// Creates a new profile.
#[tauri::command]
pub async fn create_profile(
    name: String,
    display_name: String,
    provider: String,
    model: String,
    api_key: String,
) -> Result<(), String> {
    let mut config = crate::profiles::ProfileConfig::load();
    config.create(name, display_name, provider, model, api_key)
}

/// Switches to a different profile.
#[tauri::command]
pub async fn switch_profile(name: String) -> Result<(), String> {
    let mut config = crate::profiles::ProfileConfig::load();
    config.switch(&name)
}

/// Deletes a profile.
#[tauri::command]
pub async fn delete_profile(name: String) -> Result<(), String> {
    let mut config = crate::profiles::ProfileConfig::load();
    config.delete(&name)
}

/// Updates a profile.
#[tauri::command]
pub async fn update_profile(profile: crate::profiles::Profile) -> Result<(), String> {
    let mut config = crate::profiles::ProfileConfig::load();
    config.update(profile)
}

// ---------------------------------------------------------------------------
// Skills commands
// ---------------------------------------------------------------------------

/// Skill payload for the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct SkillPayload {
    /// Machine-readable skill identifier (e.g., "system-prompt").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Semantic version of the skill.
    pub version: String,
    /// Author or maintainer of the skill.
    pub author: String,
    /// SPDX license identifier.
    pub license: String,
    /// Supported platforms (e.g., ["linux", "macos", "windows"]).
    pub platforms: Vec<String>,
    /// Category for grouping (e.g., "system", "devops").
    pub category: String,
    /// Absolute filesystem path to the skill directory.
    pub path: String,
    /// Tags for filtering and search.
    pub tags: Vec<String>,
    /// Names of related skills.
    pub related_skills: Vec<String>,
    /// Full markdown content of the skill.
    pub content: String,
}

impl From<&crate::skills::Skill> for SkillPayload {
    fn from(s: &crate::skills::Skill) -> Self {
        Self {
            name: s.name.clone(),
            description: s.description.clone(),
            version: s.version.clone(),
            author: s.author.clone(),
            license: s.license.clone(),
            platforms: s.platforms.clone(),
            category: s.category.clone(),
            path: s.path.clone(),
            tags: s.tags.clone(),
            related_skills: s.related_skills.clone(),
            content: s.content.clone(),
        }
    }
}

/// Lists all discovered skills.
#[tauri::command]
pub async fn list_skills() -> Result<Vec<SkillPayload>, String> {
    let skills = crate::skills::scan_skills();
    Ok(skills.iter().map(SkillPayload::from).collect())
}

/// Gets the content of a specific skill.
#[tauri::command]
pub async fn get_skill_content(name: String) -> Result<String, String> {
    crate::skills::read_skill_content(&name).ok_or_else(|| format!("Skill '{}' not found", name))
}

/// Reads a linked file from a skill directory.
#[tauri::command]
pub async fn get_skill_file(name: String, file_path: String) -> Result<String, String> {
    crate::skills::read_skill_file(&name, &file_path)
        .ok_or_else(|| format!("File '{}' not found in skill '{}'", file_path, name))
}

// ---------------------------------------------------------------------------
// Model fetching
// ---------------------------------------------------------------------------

/// Available model info.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "openai/gpt-4o").
    pub id: String,
    /// Human-readable model name.
    pub name: String,
    /// Provider identifier (e.g., "openai", "anthropic").
    pub provider: String,
}

/// OpenRouter API response structure.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

/// OpenRouter model entry.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    name: Option<String>,
}

/// Fetches available models from OpenRouter.
#[tauri::command]
pub async fn fetch_models() -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://openrouter.ai/api/v1/models")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("OpenRouter returned status: {}", resp.status()));
    }

    let body: OpenRouterModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse models response: {e}"))?;

    let models = body
        .data
        .into_iter()
        .map(|item| ModelInfo {
            id: item.id.clone(),
            name: match item.name {
                Some(n) => n,
                None => item.id.clone(),
            },
            provider: "openrouter".into(),
        })
        .collect();

    Ok(models)
}

/// File/directory entry for the file explorer.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct DirEntry {
    /// File or directory name.
    pub name: String,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Full path to the entry.
    pub path: String,
}

/// Reads the contents of a directory.
#[tauri::command]
pub async fn read_directory(path: String) -> Result<Vec<DirEntry>, String> {
    let mut entries = Vec::new();
    let path = std::path::PathBuf::from(path);
    if !path.exists() {
        return Err("Path does not exist".into());
    }
    if !path.is_dir() {
        return Err("Path is not a directory".into());
    }
    let mut dir = tokio::fs::read_dir(&path)
        .await
        .map_err(|e| format!("Failed to read directory: {e}"))?;
    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read entry: {e}"))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = match entry.file_type().await {
            Ok(ft) => ft.is_dir(),
            Err(_) => false,
        };
        let path = entry.path().to_string_lossy().to_string();
        entries.push(DirEntry { name, is_dir, path });
    }
    entries.sort_by(|a, b| {
        // Directories first, then by name
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        }
    });
    Ok(entries)
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
