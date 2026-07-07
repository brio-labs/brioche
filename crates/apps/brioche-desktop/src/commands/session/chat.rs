//! Chat messaging and stream forwarding commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_core::{ChatMessage, EngineInput};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::{DesktopState, persist_session};

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
pub(super) fn emit_system(app: &AppHandle, content: impl Into<String>) {
    let _ = app.emit(
        "chat-message",
        ChatMessagePayload::from(ChatMessage::System {
            content: content.into(),
        }),
    );
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
        return super::slash::handle_slash_command(&app, state, cmd, trimmed).await;
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
pub async fn get_messages(
    state: State<'_, DesktopState>,
) -> Result<Vec<ChatMessagePayload>, String> {
    get_messages_impl(state.inner()).await
}

pub(super) async fn get_messages_impl(
    state: &DesktopState,
) -> Result<Vec<ChatMessagePayload>, String> {
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
pub async fn clear_messages(state: State<'_, DesktopState>) -> Result<(), String> {
    rebuild_current_session(state.inner()).await?;
    Ok(())
}
pub(super) async fn rebuild_current_session(state: &DesktopState) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::session::test_support::{test_state, wait_for_system_message};

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
}
