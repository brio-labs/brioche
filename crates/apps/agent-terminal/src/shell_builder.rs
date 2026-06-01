//! Builds a complete `BriocheShell` with all its components.
//!
//! This module is shared between interactive and headless modes.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_core::{ActiveToolCall, ChatMessage, ToolResultDTO};
use brioche_plugin_kit::PluginBuilder;
use brioche_provider_openai::{OpenAiLlmClient, SharedHistory, ShellEvent};
use brioche_shell_persistence::{RedbStorage, SessionHeadDTO, SessionStore, SessionStoreEntry};
use brioche_shell_runtime::{AllowList, SystemToolExecutor};
use brioche_shell_runtime::{BriocheShell, DefaultEffectExecutor, ShellConfig, ToolExecutor};
use brioche_tool_fetch::FetchUrlTool;
use brioche_tool_listdir::ListDirTool;
use brioche_tool_readfile::ReadFileTool;
use brioche_tool_shell::ExecuteCommandTool;
use brioche_tool_writefile::WriteFileTool;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::config::CliConfig;

/// `ToolExecutor` decorator that synchronizes results with the LLM
/// client history mirror.
///
/// Separates the "tool execution" concern (delegated to `inner`)
/// from the "LLM history synchronization" concern (implemented
/// here). The decorator is generic over `T: ToolExecutor` to
/// enable chainable composition.
///
/// Refs: I-Shell-ToolResult-PassThrough, I-Comp-Atomic-Concern
pub struct HistorySyncDecorator<T: ToolExecutor> {
    inner: T,
    llm: OpenAiLlmClient,
}

impl<T: ToolExecutor> HistorySyncDecorator<T> {
    /// Wraps an existing `ToolExecutor` with LLM history sync.
    pub fn new(inner: T, llm: OpenAiLlmClient) -> Self {
        Self { inner, llm }
    }
}

#[async_trait::async_trait]
impl<T: ToolExecutor> ToolExecutor for HistorySyncDecorator<T> {
    async fn execute(&self, call: &ActiveToolCall, cancel: CancellationToken) -> ToolResultDTO {
        let result = self.inner.execute(call, cancel).await;
        self.llm
            .push_tool_results(std::slice::from_ref(&result))
            .await;
        result
    }
}

/// Builds a complete `BriocheShell` with all its components.
///
/// This function is reusable for creating multiple shells
/// (multi-session) or a headless shell.
pub async fn build_shell(
    session_id: impl Into<String>,
    cli_config: &CliConfig,
    with_confirm: bool,
    redb_storage: RedbStorage,
    session_store: SessionStore,
    initial_history: Option<Vec<ChatMessage>>,
    initial_head: Option<SessionHeadDTO>,
) -> (
    BriocheShell,
    OpenAiLlmClient,
    broadcast::Receiver<ShellEvent>,
    SharedHistory,
) {
    type ConfirmHandler = Option<std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>>;
    let confirm_handler: ConfirmHandler = if with_confirm {
        Some(std::sync::Arc::new(|cmd: &str| {
            use std::io::Write;
            print!(
                "\n{} Command '{}' is not in the allow-list. Execute ? [y/N] ",
                nu_ansi_term::Color::Yellow.paint("⚠"),
                cmd
            );
            let _ = std::io::stdout().flush();
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input).is_err() {
                return false;
            }
            matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
        }))
    } else {
        None
    };

    let mut exec_tool = ExecuteCommandTool::with_allow_list(AllowList::default());
    if let Some(handler) = confirm_handler {
        exec_tool = exec_tool.with_confirm_handler(handler);
    }

    let tool_executor = SystemToolExecutor::new()
        .with_tool(ReadFileTool)
        .with_tool(WriteFileTool)
        .with_tool(ListDirTool)
        .with_tool(exec_tool)
        .with_tool(FetchUrlTool);

    let (llm_client, llm_rx, history) = OpenAiLlmClient::new(cli_config.openai.clone());

    let schemas = tool_executor.schema_json();
    llm_client.set_tools_schema(schemas).await;

    let notifying_tools = HistorySyncDecorator::new(tool_executor, llm_client.clone());

    let effect_executor =
        DefaultEffectExecutor::new(notifying_tools, llm_client.clone(), redb_storage.clone())
            .with_ui_tx(llm_client.ui_tx());

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
            let (engine, mut session) = PluginBuilder::standard()
                .build_with_session(&session_id)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to build engine: {e}");
                    std::process::exit(1);
                });
            if let Some(head) = initial_head {
                session = head.to_session(initial_history_for_factory.unwrap_or_default());
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
