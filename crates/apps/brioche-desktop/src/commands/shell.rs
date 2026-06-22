//! Shell construction and configuration for the desktop app.
//!
//! Reuses `agent-terminal`'s shell builder pattern but adapted
//! for Tauri's async runtime.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::{Arc, Mutex};

use brioche_core::ChatMessage;
use brioche_plugin_kit::PluginBuilder;
use brioche_provider_openai::{
    HistoryTransform, LlmChunk, OpenAiConfig, OpenAiLlmClient, SharedHistory,
};
use brioche_shell_persistence::extensions::context::{
    CompressorContextEngine, ContextEngine, ContextEngineInput,
};
use brioche_shell_persistence::{
    ExtensionRegistry, MemoryProvider, RedbStorage, SessionStore, SessionStoreEntry, Settings,
    UserDefinedTool,
};
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
///
/// # Complexity
/// O(1) atomic lock access.
///
/// # Panic / Safety
/// Never panics. Returns current time if lock is poisoned.
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
///
/// # Complexity
/// Struct contains OpenAiConfig strings. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug)]
pub struct DesktopConfig {
    /// OpenAI provider configuration.
    pub openai: OpenAiConfig,
    /// Tick interval in milliseconds.
    pub tick_interval_ms: u64,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self::from_settings(&Settings::load())
    }
}

impl DesktopConfig {
    /// Builds a desktop configuration from modular settings.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(S) where S is settings size. Inspects environment variables.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn from_settings(settings: &Settings) -> Self {
        let api_key = if settings.api_key().is_empty() {
            std::env::var("BRIOCHE_API_KEY").map_or(String::new(), |v| v)
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
///
/// # Complexity
/// Struct contains Arc-wrapped services and settings. O(1) clone.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone)]
pub struct ShellFactory {
    /// Redb storage for session persistence.
    pub redb: RedbStorage,
    /// Session store for in-memory state.
    pub store: SessionStore,
    /// CLI configuration (provider, timeouts, etc.).
    pub config: DesktopConfig,
    /// Loaded desktop extensions (context engine, memory, tools, skills, ...).
    pub extensions: ExtensionRegistry,
    /// User settings snapshot at shell creation time.
    pub settings: Settings,
    /// Shared slot for the last context-engine note.
    pub last_context_note: Arc<Mutex<Option<String>>>,
}

/// Handle to a running shell and its LLM broadcast channel.
///
/// The frontend receives `LlmChunk` events via Tauri's event system.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Wraps running task handles and channels. O(1).
///
/// # Panic / Safety
/// Never panics.
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

/// Builds a history transform from settings and the extension registry.
///
/// The transform is applied to the conversational mirror right before the
/// LLM request is built. It leaves the mirror untouched so the UI and
/// persistence still see the full conversation.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn build_history_transform(
    settings: Settings,
    extensions: ExtensionRegistry,
    last_context_note: Arc<Mutex<Option<String>>>,
) -> HistoryTransform {
    Arc::new(move |history: &[ChatMessage]| {
        let mut working: Vec<ChatMessage> = history.to_vec();

        // ------------------------------------------------------------------
        // Memory recall: prepend relevant entries as a system message.
        // ------------------------------------------------------------------
        let active_providers = settings.active_memory_providers();
        let summary = working
            .iter()
            .rev()
            .find_map(|m| match m {
                ChatMessage::User { content } => Some(content.as_str()),
                _ => None,
            })
            .map_or("", |v| v); // Allowed by design: empty summary is a valid default.
        let mut memory_notes: Vec<String> = Vec::new();
        for provider in extensions.memory_providers() {
            let id = provider.metadata().id;
            if !active_providers.contains(&id) {
                continue;
            }
            match provider.recall(summary, 3) {
                Ok(entries) => {
                    for entry in entries {
                        memory_notes.push(format!("{}: {}", entry.key, entry.value));
                    }
                }
                Err(err) => tracing::warn!("Memory recall failed for {}: {}", id, err),
            }
        }
        if !memory_notes.is_empty() {
            let insert_idx = working
                .iter()
                .position(|m| !matches!(m, ChatMessage::System { .. }))
                .map_or(0, |v| v); // Allowed by design: no non-system messages means prepend at start.
            working.insert(
                insert_idx,
                ChatMessage::System {
                    content: format!("Relevant memory context:\n{}", memory_notes.join("\n")),
                },
            );
        }

        // ------------------------------------------------------------------
        // Context engine: compress when the budget is exceeded.
        // ------------------------------------------------------------------
        let enabled = match settings.get("context.enabled") {
            Some(serde_json::Value::Bool(b)) => b,
            _ => true,
        };
        if enabled {
            let trigger = settings
                .get("context.trigger_percentage")
                .and_then(|v| v.as_u64())
                .map_or(75, |v| v as u8);
            let target = settings
                .get("context.target_percentage")
                .and_then(|v| v.as_u64())
                .map_or(50, |v| v as u8);
            let preserve = settings
                .get("context.preserve_recent")
                .and_then(|v| v.as_u64())
                .map_or(6, |v| v as usize);
            let engine = CompressorContextEngine::new(trigger, target, preserve);
            let input = ContextEngineInput {
                history: &working,
                context_window: settings.context_window(),
                estimated_tokens: CompressorContextEngine::estimate_tokens(&working),
            };
            let output = engine.process(input);
            if let Some(note) = output.note
                && let Ok(mut guard) = last_context_note.lock()
            {
                *guard = Some(note);
            }
            working = output.messages;
        }

        working
    })
}

/// A wrapper tool that exposes a `MemoryProvider`'s custom tool schemas to the LLM.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Wraps provider trait object and JSON schema. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone)]
pub struct MemoryProviderTool {
    provider: Arc<dyn MemoryProvider>,
    schema: serde_json::Value,
}

impl std::fmt::Debug for MemoryProviderTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryProviderTool")
            .field("schema", &self.schema)
            .finish()
    }
}

impl MemoryProviderTool {
    /// Creates a new `MemoryProviderTool` wrapper.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1) creation.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn new(provider: Arc<dyn MemoryProvider>, schema: serde_json::Value) -> Self {
        Self { provider, schema }
    }
}

#[async_trait::async_trait]
impl brioche_tools_system::SystemTool for MemoryProviderTool {
    fn name(&self) -> String {
        match self.schema["function"]["name"].as_str() {
            Some(s) => s.to_string(),
            None => String::new(),
        }
    }

    fn description(&self) -> String {
        match self.schema["function"]["description"].as_str() {
            Some(s) => s.to_string(),
            None => String::new(),
        }
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.schema["function"]["parameters"].clone()
    }

    async fn run(
        &self,
        args: serde_json::Value,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> Result<String, brioche_tools_system::ToolError> {
        let name = self.name();
        self.provider
            .handle_tool_call(&name, args)
            .map_err(|err| brioche_tools_system::ToolError::Io(std::io::Error::other(err)))
    }
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
///
/// # Complexity
/// O(T) where T is the number of tools registered. Spawns asynchronous tasks in the background.
///
/// # Panic / Safety
/// Panics if called outside of a Tokio runtime context since it spawns tokio tasks.
pub fn build_shell(session_id: impl Into<String>, factory: &ShellFactory) -> ShellHandle {
    let workspace_path = factory.settings.working_dir();
    let workspace = if workspace_path.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(&workspace_path))
    };

    let exec_tool = ExecuteCommandTool::new()
        .with_policy(SandboxPolicy::Permissive)
        .with_default_cwd(workspace_path);

    let mut tool_executor = SystemToolExecutor::new()
        .with_tool(ReadFileTool::new(workspace.clone()))
        .with_tool(WriteFileTool::new(workspace.clone()))
        .with_tool(ListDirTool::new(workspace))
        .with_tool(exec_tool)
        .with_tool(FetchUrlTool);

    // Register user-defined tools from all tool providers.
    for provider in factory.extensions.tool_providers() {
        for user_tool in provider.user_tools() {
            tool_executor = tool_executor.with_tool(UserDefinedTool::new(user_tool));
        }
    }

    // Register tools from active memory providers.
    let active_memory_providers = factory.settings.active_memory_providers();
    for provider in factory.extensions.memory_providers() {
        let id = provider.metadata().id;
        if active_memory_providers.contains(&id) {
            for schema in provider.tool_schemas() {
                tool_executor =
                    tool_executor.with_tool(MemoryProviderTool::new(Arc::clone(provider), schema));
            }
        }
    }

    let (llm_client, llm_rx, history) = OpenAiLlmClient::new(factory.config.openai.clone());
    llm_client.set_history_transform(Some(build_history_transform(
        factory.settings.clone(),
        factory.extensions.clone(),
        Arc::clone(&factory.last_context_note),
    )));

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
        DefaultEffectExecutor::new(tool_executor, llm_client.clone(), factory.redb.clone());

    // Session callback — snapshot after each transition.
    let store_for_callback = Arc::clone(&factory.store);
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
            tick_interval_ms: factory.config.tick_interval_ms,
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
        let factory = ShellFactory {
            redb,
            store,
            config,
            extensions: ExtensionRegistry::default_set(),
            settings: Settings::default(),
            last_context_note: Arc::new(Mutex::new(None)),
        };
        let handle = build_shell("test-session", &factory);
        assert_eq!(handle.llm_rx.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a running AMP memory server at 127.0.0.1:9471"]
    #[allow(
        clippy::unwrap_used,
        clippy::disallowed_methods,
        clippy::disallowed_types
    )]
    async fn test_memory_provider_tool_execution() -> Result<(), Box<dyn std::error::Error>> {
        use brioche_shell_persistence::{AmpMemoryEndpoint, AmpMemoryProvider};
        use brioche_tools_system::SystemTool;

        let mut extensions = ExtensionRegistry::new();
        let amp_endpoint = AmpMemoryEndpoint {
            id: "memory-amp-1".into(),
            name: "Local mem0".into(),
            url: "http://127.0.0.1:9471".into(),
            api_key: None,
            scope: None,
        };
        let provider = Arc::new(AmpMemoryProvider::new(amp_endpoint));
        extensions.register_memory_provider(provider.clone());

        let mut found_store = None;
        let mut found_recall = None;
        for schema in provider.tool_schemas() {
            let tool = MemoryProviderTool::new(provider.clone(), schema);
            if tool.name() == "memory-amp-1_store" {
                found_store = Some(tool);
            } else if tool.name() == "memory-amp-1_recall" {
                found_recall = Some(tool);
            }
        }

        let store_tool = found_store.ok_or("memory-amp-1_store tool not found")?;
        let recall_tool = found_recall.ok_or("memory-amp-1_recall tool not found")?;

        let cancel = tokio_util::sync::CancellationToken::new();
        let store_args = serde_json::json!({
            "content": "My favorite food is brioche."
        });

        let store_result = store_tool.run(store_args, cancel.clone()).await?;
        assert_eq!(store_result, "{\"status\":\"stored\"}");

        let recall_args = serde_json::json!({
            "query": "favorite food"
        });
        let recall_result = recall_tool.run(recall_args, cancel).await?;

        let recalled_value: Vec<String> = serde_json::from_str(&recall_result)?;
        assert!(recalled_value.iter().any(|v| v.contains("brioche")));

        Ok(())
    }
}
