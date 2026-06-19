//! Builds a complete `BriocheShell` with all its components.
//!
//! This module is shared between interactive and headless modes.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_core::ChatMessage;
use brioche_plugin_kit::PluginBuilder;
use brioche_provider_openai::{LlmChunk, OpenAiLlmClient, SharedHistory};
use brioche_shell_persistence::{RedbStorage, SessionHeadDTO, SessionStore, SessionStoreEntry};
use brioche_shell_runtime::{BriocheShell, DefaultEffectExecutor, ShellConfig};
use brioche_tools_system::{
    ExecuteCommandTool, FetchUrlTool, ListDirTool, ReadFileTool, SandboxPolicy, SystemToolExecutor,
    WriteFileTool,
};
use tokio::sync::broadcast;

use crate::CliConfig;

/// Builds a complete `BriocheShell` with all its components.
///
/// This function is reusable for creating multiple shells
/// (multi-session) or a headless shell.
/// Refs: docs/SPECS.md §Book IV
pub fn build_shell(
    session_id: impl Into<String>,
    cli_config: &CliConfig,
    redb_storage: RedbStorage,
    session_store: SessionStore,
    initial_history: Option<Vec<ChatMessage>>,
    initial_head: Option<SessionHeadDTO>,
) -> (
    BriocheShell,
    OpenAiLlmClient,
    broadcast::Receiver<LlmChunk>,
    SharedHistory,
) {
    // Agent-terminal runs without a permission system — all commands
    // are executed directly. This is intentional: the user is the
    // human-in-the-loop and controls the terminal.
    let exec_tool = ExecuteCommandTool::new().with_policy(SandboxPolicy::Permissive);

    let tool_executor = SystemToolExecutor::new()
        .with_tool(ReadFileTool::default())
        .with_tool(WriteFileTool::default())
        .with_tool(ListDirTool::default())
        .with_tool(exec_tool)
        .with_tool(FetchUrlTool);

    let (llm_client, llm_rx, history) = OpenAiLlmClient::new(cli_config.openai.clone());

    // Inject default system prompt into the LLM client's history mirror.
    // This instructs the model to use available tools rather than
    // emitting file contents directly in its response.
    let llm_for_prompt = llm_client.clone();
    tokio::spawn(async move {
        llm_for_prompt.push_message(ChatMessage::System {
            content: "You are a helpful AI coding assistant with access to filesystem tools. \
CRITICAL RULES: \
1. When the user asks you to create, write, or modify ANY file, you MUST use the write_file tool. \
2. NEVER output file contents directly in your response text — ALWAYS use the write_file tool with the full content. \
3. For large files, you may use multiple write_file calls with append=true after the first call. \
4. Use read_file before modifying existing files. \
5. Use execute_command for shell commands. \
6. Use list_dir to explore directories. \
7. If you need to fetch content from the web, use fetch_url. \
8. After using write_file, the tool result will confirm success. Do not read the file back unless the user asks.".into(),
        }).await;
    });

    let schemas = tool_executor.schema_json();
    let llm_for_schema = llm_client.clone();
    tokio::spawn(async move {
        // `set_tools_schema` is infallible (writes to an RwLock).
        // The task runs in background so startup is not blocked.
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
    let initial_history_for_factory = initial_history.clone();
    let shell = BriocheShell::new(
        move || {
            let (engine, mut session) = PluginBuilder::standard().build_with_session(&session_id);
            if let Some(head) = initial_head {
                session =
                    head.to_session(initial_history_for_factory.map_or(Default::default(), |v| v));
            } else if let Some(hist) = initial_history_for_factory {
                session.history = hist;
            }
            (engine, session)
        },
        ShellConfig {
            engine_channel_capacity: 256,
            tick_interval_ms: cli_config.tick_interval_ms,
            max_concurrent_effects: 32,
            persistence_mode: brioche_shell_runtime::PersistenceMode::Async,
            transition_journal_enabled: false,
        },
        effect_executor,
        Some(session_callback),
    );

    (shell, llm_client, llm_rx, history_clone)
}
