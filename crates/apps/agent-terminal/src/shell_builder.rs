//! Builds a complete `BriocheShell` with all its components.
//!
//! This module is shared between interactive and headless modes.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::io::{BufRead, Write};
use std::sync::Arc;

use brioche_core::ChatMessage;
use brioche_provider_openai::{LlmChunk, OpenAiLlmClient, SharedHistory};
use brioche_shell_builder::{ShellBuilder, session_factory_with_head};
use brioche_shell_persistence::{RedbStorage, SessionHeadDTO, SessionStore};
use brioche_shell_runtime::{BriocheShell, ShellError};
use brioche_tools_system::{
    AllowList, ExecuteCommandTool, FetchUrlTool, ListDirTool, ReadFileTool, SandboxPolicy,
    SystemToolExecutor, WriteFileTool,
};
use tokio::sync::broadcast;

use crate::CliConfig;

/// Execution mode for a shell session.
///
/// Determines the default sandbox policy for shell commands and
/// whether interactive confirmation prompts are appropriate.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellMode {
    /// Full REPL with a human-in-the-loop. Commands outside the
    /// default allow-list trigger an interactive confirmation prompt.
    Interactive,
    /// Single prompt, non-interactive mode. Only allow-listed commands
    /// run unless the permissive opt-in is enabled.
    Headless,
}

/// Returns the sandbox policy selected for the given terminal mode.
///
/// The default is the system allow-list. Permissive mode is only
/// selected when the user explicitly opts in via CLI flag or env var.
/// Refs: docs/SPECS.md §Book III-C
pub fn sandbox_policy_for(cli_config: &CliConfig, _mode: ShellMode) -> SandboxPolicy {
    if cli_config.permissive_shell {
        SandboxPolicy::Permissive
    } else {
        SandboxPolicy::AllowList(AllowList::default())
    }
}

/// Builds a complete `BriocheShell` with all its components.
///
/// This function is reusable for creating multiple shells
/// (multi-session) or a headless shell. It is async and must be awaited
/// from a Tokio runtime context.
///
/// # Errors
/// Returns a `ShellError` if the LLM client fails to initialize.
///
/// Refs: docs/SPECS.md §Book IV
///
/// # Complexity
/// O(T) where T is the number of tools registered. Performs a bounded amount
/// of async initialization before returning.
///
/// # Cancel safety
/// This future delegates to `ShellBuilder::build`. Dropping it before
/// completion leaves the caller without a shell, but does not leave partial
/// state in the LLM client.
///
/// # Panic / Safety
/// Never panics.
pub async fn build_shell(
    session_id: impl Into<String>,
    cli_config: &CliConfig,
    mode: ShellMode,
    redb_storage: RedbStorage,
    session_store: SessionStore,
    initial_history: Option<Vec<ChatMessage>>,
    initial_head: Option<SessionHeadDTO>,
) -> Result<
    (
        BriocheShell,
        OpenAiLlmClient,
        broadcast::Receiver<LlmChunk>,
        SharedHistory,
    ),
    ShellError,
> {
    let exec_tool = match sandbox_policy_for(cli_config, mode) {
        SandboxPolicy::Permissive => {
            tracing::warn!(
                "permissive shell execution enabled; all commands will run without confirmation"
            );
            ExecuteCommandTool::new().with_policy(SandboxPolicy::Permissive)
        }
        SandboxPolicy::AllowList(list) => {
            let tool = ExecuteCommandTool::new().with_policy(SandboxPolicy::AllowList(list));
            if mode == ShellMode::Interactive {
                tool.with_confirm_handler(Arc::new(prompt_confirm))
            } else {
                tool
            }
        }
        SandboxPolicy::Interactive => {
            unreachable!("agent-terminal does not use the Interactive sandbox policy directly")
        }
    };

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
    .await
}

/// Prompts the user to confirm execution of a shell command.
///
/// This is used as the `ConfirmHandler` for interactive mode. It
/// blocks on stdin, so it is called inside `tokio::task::spawn_blocking`
/// by the tool executor.
fn prompt_confirm(command: &str) -> bool {
    let mut stderr = std::io::stderr();
    let _ = writeln!(stderr, "The following command requires confirmation:");
    let _ = writeln!(stderr, "  {command}");
    let _ = write!(stderr, "Allow execution? [y/N] ");
    let _ = stderr.flush();

    let mut input = String::new();
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    let Ok(_) = handle.read_line(&mut input) else {
        return false;
    };

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}
