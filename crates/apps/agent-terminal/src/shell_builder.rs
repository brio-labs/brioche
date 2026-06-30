//! Builds a complete `BriocheShell` with all its components.
//!
//! This module is shared between interactive and headless modes.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_core::ChatMessage;
use brioche_provider_openai::{LlmChunk, OpenAiLlmClient, SharedHistory};
use brioche_shell_builder::{ShellBuilder, session_factory_with_head};
use brioche_shell_persistence::{RedbStorage, SessionHeadDTO, SessionStore};
use brioche_shell_runtime::BriocheShell;
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

    let session_id = session_id.into();
    let history_for_factory = initial_history.clone();
    ShellBuilder::new(
        &session_id,
        cli_config.openai.clone(),
        redb_storage.clone(),
        session_store,
        tool_executor,
    )
    .with_tick_interval_ms(cli_config.tick_interval_ms)
    .with_engine_factory(session_factory_with_head(
        session_id,
        redb_storage,
        history_for_factory,
        initial_head,
    ))
    .build()
}
