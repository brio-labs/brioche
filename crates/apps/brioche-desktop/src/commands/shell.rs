//! Shell construction and configuration for the desktop app.
//!
//! Reuses `agent-terminal`'s shell builder pattern but adapted
//! for Tauri's async runtime.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_core::ChatMessage;
use brioche_plugin_kit::PluginBuilder;
use brioche_provider_openai::{LlmChunk, OpenAiConfig, OpenAiLlmClient, SharedHistory};
use brioche_shell_persistence::{RedbStorage, SessionStore, SessionStoreEntry};
use brioche_shell_runtime::{BriocheShell, DefaultEffectExecutor, ShellConfig};
use brioche_tools_system::{
    ExecuteCommandTool, FetchUrlTool, ListDirTool, ReadFileTool, SandboxPolicy, SystemToolExecutor,
    WriteFileTool,
};
use lazy_static::lazy_static;
use tokio::sync::broadcast;

lazy_static! {
    /// Global session start timestamp, captured the first time a shell is built.
    static ref SESSION_START: std::sync::Mutex<u64> = std::sync::Mutex::new(system_time_secs());
}

/// Returns seconds since the UNIX epoch.
fn system_time_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}

/// Returns the timestamp when the first shell was built in this process.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn session_started_at() -> u64 {
    match SESSION_START.lock() {
        Ok(guard) => *guard,
        Err(_) => system_time_secs(),
    }
}

/// Configuration for the desktop shell.
///
/// Mirrors `agent_terminal::CliConfig` but is self-contained
/// to avoid coupling the app crate to the terminal crate.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug)]
pub struct DesktopConfig {
    /// OpenAI provider configuration.
    pub openai: OpenAiConfig,
    /// Tick interval in milliseconds.
    pub tick_interval_ms: u64,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self::from_settings(&crate::settings::Settings::load())
    }
}

impl DesktopConfig {
    /// Builds a desktop configuration from modular settings.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn from_settings(settings: &crate::settings::Settings) -> Self {
        let api_key = if settings.api_key().is_empty() {
            std::env::var("BRIOCHE_API_KEY").unwrap_or_default()
        } else {
            settings.api_key()
        };
        let model = match std::env::var("BRIOCHE_MODEL") {
            Ok(v) => v,
            Err(_) => settings.chat_model(),
        };
        let base_url = match std::env::var("BRIOCHE_BASE_URL") {
            Ok(v) => v,
            Err(_) => settings.base_url(),
        };
        let max_tokens = settings.max_tokens();
        let reasoning_enabled = match settings.get("chat.reasoning_enabled") {
            Some(serde_json::Value::Bool(b)) => b,
            _ => false,
        };
        let reasoning_effort = if reasoning_enabled {
            let effort = match settings.get("chat.reasoning_effort") {
                Some(serde_json::Value::String(s)) => s,
                _ => "medium".to_string(),
            };
            Some(effort)
        } else {
            std::env::var("BRIOCHE_REASONING_EFFORT").ok()
        };

        Self {
            openai: OpenAiConfig {
                api_key,
                model,
                base_url,
                max_tokens,
                timeout_ms: 120_000,
                reasoning_effort,
            },
            tick_interval_ms: 1000,
        }
    }
}

/// Dependencies needed to create new shells.
///
/// Shared between the main loop, slash command handlers, and
/// session management.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone)]
pub struct ShellFactory {
    /// Redb storage for session persistence.
    pub redb: RedbStorage,
    /// Session store for in-memory state.
    pub store: SessionStore,
    /// CLI configuration (provider, timeouts, etc.).
    pub config: DesktopConfig,
}

/// Handle to a running shell and its LLM broadcast channel.
///
/// The frontend receives `LlmChunk` events via Tauri's event system.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct ShellHandle {
    /// The shell instance.
    pub shell: BriocheShell,
    /// LLM client for pushing messages.
    pub llm: OpenAiLlmClient,
    /// Broadcast receiver for LLM chunks (frontend listens to events).
    pub llm_rx: broadcast::Receiver<LlmChunk>,
    /// Shared history mirror.
    pub history: SharedHistory,
}

/// Builds a complete `ShellHandle` with all components.
///
/// This is the desktop equivalent of `agent_terminal::shell_builder::build_shell`.
///
/// **Critical:** This function must be called from within an async runtime
/// context (e.g., inside a Tauri command) because it spawns background tasks
/// via `tokio::spawn`. Calling it from a synchronous context will panic.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn build_shell(
    session_id: impl Into<String>,
    config: &DesktopConfig,
    redb_storage: RedbStorage,
    session_store: SessionStore,
) -> ShellHandle {
    let exec_tool = ExecuteCommandTool::new().with_policy(SandboxPolicy::Permissive);

    let tool_executor = SystemToolExecutor::new()
        .with_tool(ReadFileTool)
        .with_tool(WriteFileTool)
        .with_tool(ListDirTool)
        .with_tool(exec_tool)
        .with_tool(FetchUrlTool);

    let (llm_client, llm_rx, history) = OpenAiLlmClient::new(config.openai.clone());

    // Inject default system prompt.
    let llm_for_prompt = llm_client.clone();
    tokio::spawn(async move {
        llm_for_prompt
            .push_message(ChatMessage::System {
                content: "You are a helpful AI coding assistant with access to filesystem tools. \
CRITICAL RULES: \
1. When the user asks you to create, write, or modify ANY file, you MUST use the write_file tool. \
2. NEVER output file contents directly in your response text — ALWAYS use the write_file tool with the full content. \
3. For large files, you may use multiple write_file calls with append=true after the first call. \
4. Use read_file before modifying existing files. \
5. Use execute_command for shell commands. \
6. Use list_dir to explore directories. \
7. If you need to fetch content from the web, use fetch_url. \
8. After using write_file, the tool result will confirm success. Do not read the file back unless the user asks."
                    .into(),
            })
            .await;
    });

    let schemas = tool_executor.schema_json();
    let llm_for_schema = llm_client.clone();
    tokio::spawn(async move {
        llm_for_schema.set_tools_schema(schemas).await;
    });

    let effect_executor =
        DefaultEffectExecutor::new(tool_executor, llm_client.clone(), redb_storage.clone());

    // Session callback — snapshot after each transition.
    let store_for_callback = Arc::clone(&session_store);
    let session_callback: brioche_shell_runtime::SessionCallback =
        Box::new(move |session: &brioche_core::Session| {
            let head = brioche_shell_persistence::SessionHeadDTO::from_session(session);
            let entry = SessionStoreEntry {
                head,
                messages: session.history.clone(),
            };
            if let Ok(mut store) = store_for_callback.try_write() {
                store.insert(session.id.clone(), entry);
            }
        });

    let session_id = session_id.into();
    let history_clone = Arc::clone(&history);
    let shell = BriocheShell::new(
        move || {
            let (engine, session) = PluginBuilder::standard().build_with_session(&session_id);
            (engine, session)
        },
        ShellConfig {
            engine_channel_capacity: 256,
            tick_interval_ms: config.tick_interval_ms,
            max_concurrent_effects: 32,
            persistence_mode: brioche_shell_runtime::PersistenceMode::Async,
            transition_journal_enabled: false,
        },
        effect_executor,
        Some(session_callback),
    );

    ShellHandle {
        shell,
        llm: llm_client,
        llm_rx,
        history: history_clone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that `DesktopConfig::default()` produces a valid config
    /// without panicking.
    #[test]
    fn desktop_config_default() {
        let config = DesktopConfig::default();
        assert_eq!(config.tick_interval_ms, 1000);
        assert_eq!(config.openai.timeout_ms, 120_000);
    }

    /// Verifies that `build_shell` constructs a shell without panicking.
    ///
    /// Note: This test runs inside a tokio runtime so `tokio::spawn` works.
    #[tokio::test]
    async fn build_shell_smoke() {
        let config = DesktopConfig {
            openai: OpenAiConfig {
                api_key: String::new(),
                model: "gpt-4o-mini".into(),
                base_url: "https://api.openai.com/v1".into(),
                max_tokens: 4096,
                timeout_ms: 120_000,
                reasoning_effort: None,
            },
            tick_interval_ms: 1000,
        };
        let redb_result = RedbStorage::new(
            "/tmp/brioche-desktop-test.redb",
            brioche_shell_persistence::new_session_store(),
        );
        assert!(redb_result.is_ok(), "Failed to create RedbStorage for test");
        let redb = match redb_result {
            Ok(r) => r,
            Err(_) => return,
        };
        let store = brioche_shell_persistence::new_session_store();
        let handle = build_shell("test-session", &config, redb, store);
        assert_eq!(handle.llm_rx.len(), 0);
    }
}
