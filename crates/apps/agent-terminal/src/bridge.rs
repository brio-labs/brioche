//! Async bridge task: receives REPL lines and routes them to the
//! current shell or executes slash commands.
//!
//! The bridge is the only place where user commands are interpreted.
//! It is entirely stateless: state (current session, history, etc.)
//! lives in `SessionManager`.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_core::{ChatMessage, EngineInput};
use brioche_provider_openai::OpenAiLlmClient;
use brioche_reedline::SessionManager;
use brioche_shell_persistence::{RedbStorage, SessionStore};
use nu_ansi_term::{Color, Style};
use reedline::ExternalPrinter;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::CliConfig;
use crate::shell_builder::build_shell;

/// Dependencies needed to create new shells (shared between
/// `run`, `handle_slash_command`, and `handle_session_command`).
/// Refs: docs/SPECS.md §Book IV
#[derive(Clone)]
pub struct ShellFactory {
    /// Redb storage for session persistence.
    pub redb: RedbStorage,
    /// Session store for in-memory state.
    pub store: SessionStore,
    /// CLI configuration (provider, timeouts, etc.).
    pub config: CliConfig,
}

/// Main bridge loop.
///
/// Receives lines on `input_rx`, processes them, and forwards
/// messages to the current `BriocheShell`. Slash commands are
/// interpreted here; normal messages are forwarded.
///
/// # Cancel safety
/// This loop holds only local state and `RwLock` read guards across
/// await points. Dropping it stops line processing; spawned shells are
/// shut down via the `cancel` token.
pub async fn run(
    mut input_rx: mpsc::Receiver<String>,
    cancel: CancellationToken,
    printer: ExternalPrinter<String>,
    manager: Arc<RwLock<SessionManager>>,
    llm: OpenAiLlmClient,
    factory: ShellFactory,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            line = input_rx.recv() => {
                let Some(line) = line else { break; };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if let Some(cmd) = trimmed.strip_prefix('/')
                    && handle_slash_command(
                        cmd, trimmed,
                        &printer, &manager,
                        &factory, &cancel,
                    ).await {
                        continue;
                    }

                // Normal message → current session
                let shell = {
                    let mgr = manager.read().await;
                    let Some(shell) = mgr.current().cloned() else {
                        let _ = printer.print(format!(
                            "{} Invalid current session.",
                            Color::Red.paint("!")
                        ));
                        continue;
                    };
                    shell
                };

                llm.push_message(ChatMessage::User {
                    content: trimmed.to_string(),
                }).await;

                if let Err(err) = shell
                    .send_input(EngineInput::UserMessage(trimmed.to_string()))
                    .await
                {
                    eprintln!("Send error: {err}");
                }
            }
        }
    }
}

/// Processes a slash command.
///
/// Returns `true` if the command was consumed (recognized or unknown).
/// Returns `false` if it was not a slash command (should never happen here).
async fn handle_slash_command(
    cmd: &str,
    full_line: &str,
    printer: &ExternalPrinter<String>,
    manager: &Arc<RwLock<SessionManager>>,
    factory: &ShellFactory,
    cancel: &CancellationToken,
) -> bool {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    match parts.first().copied() {
        Some("quit") | Some("q") => {
            let _ = printer.print(format!("{}", Color::Green.paint("Goodbye.")));
            {
                let mgr = manager.read().await;
                for id in mgr.list() {
                    if let Some(shell) = mgr.get(id) {
                        shell.shutdown().await;
                    }
                }
            }
            cancel.cancel();
            true
        }
        Some("help") | Some("h") => {
            let _ = printer.print(print_repl_help());
            true
        }
        Some("session") if parts.len() == 1 => {
            let mgr = manager.read().await;
            let _ = printer.print(format!(
                "Current session: {}",
                Color::Green.paint(mgr.current_id())
            ));
            true
        }
        Some("session") if parts.len() >= 2 => {
            handle_session_command(&parts[1..], printer, manager, factory).await;
            true
        }
        _ => {
            let _ = printer.print(format!(
                "{} Unknown command {}",
                Color::Red.paint("!"),
                full_line
            ));
            true
        }
    }
}

async fn handle_session_command(
    args: &[&str],
    printer: &ExternalPrinter<String>,
    manager: &Arc<RwLock<SessionManager>>,
    factory: &ShellFactory,
) {
    let Some(command) = args.first().copied() else {
        let _ = printer.print(format!(
            "{} /session requires a sub-command",
            Color::Red.paint("!")
        ));
        return;
    };
    match command {
        "new" => {
            let new_id = format!(
                "session-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .map_or(0, |v| v)
            );
            let (new_shell, _new_llm, _new_rx, _history) = build_shell(
                &new_id,
                &factory.config,
                factory.redb.clone(),
                Arc::clone(&factory.store),
                None,
                None,
            );
            {
                let mut mgr = manager.write().await;
                mgr.insert(new_id.clone(), new_shell);
                mgr.switch(&new_id);
            }
            let _ = printer.print(format!("New session: {}", Color::Green.paint(&new_id)));
        }
        "list" => {
            let mgr = manager.read().await;
            let mut lines = vec![format!("{}", Style::new().bold().paint("Sessions:"))];
            for id in mgr.list() {
                let marker = if *id == mgr.current_id() {
                    " → "
                } else {
                    "   "
                };
                lines.push(format!("{}{}", marker, id));
            }
            let _ = printer.print(lines.join("\n"));
        }
        "load" => {
            let Some(id) = args.get(1).copied() else {
                let _ = printer.print(format!(
                    "{} /session load requires a session id",
                    Color::Red.paint("!")
                ));
                return;
            };
            let head = match factory.redb.load_session(id).await {
                Ok(Some(h)) => h,
                Ok(None) => {
                    let _ = printer.print(format!(
                        "{} Session '{}' not found.",
                        Color::Red.paint("!"),
                        id
                    ));
                    return;
                }
                Err(err) => {
                    let _ = printer.print(format!("{} Load error: {err}", Color::Red.paint("!")));
                    return;
                }
            };
            let messages = match factory.redb.load_messages_for_session(id).await {
                Ok(msgs) => msgs.into_iter().map(|(_, m)| m).collect(),
                Err(err) => {
                    let _ = printer.print(format!(
                        "{} Load messages error: {err}",
                        Color::Red.paint("!")
                    ));
                    return;
                }
            };
            let (new_shell, _new_llm, _new_rx, _history) = build_shell(
                id,
                &factory.config,
                factory.redb.clone(),
                Arc::clone(&factory.store),
                Some(messages),
                Some(head),
            );
            {
                let mut mgr = manager.write().await;
                mgr.insert(id.to_string(), new_shell);
                mgr.switch(id);
            }
            let _ = printer.print(format!("Session '{}' loaded.", Color::Green.paint(id)));
        }
        other => {
            let _ = printer.print(format!(
                "{} Unknown /session {other} command",
                Color::Red.paint("!")
            ));
        }
    }
}

/// Help text displayed by `/help`.
/// Refs: docs/SPECS.md §Book IV
pub fn print_repl_help() -> String {
    let mut lines = Vec::new();
    lines.push(format!("{}", Style::new().bold().paint("Commands:")));
    lines.push("  <text>               Send a message to the LLM".into());
    lines.push("  /help                Show this help".into());
    lines.push("  /quit                Exit".into());
    lines.push("  /session             Current session".into());
    lines.push("  /session new         Create a new session".into());
    lines.push("  /session list        List sessions".into());
    lines.push("  /session load <id>   Load a persisted session".into());
    lines.push(String::new());
    lines.push(format!("{}", Style::new().bold().paint("Shortcuts:")));
    lines.push("  Ctrl+C               Cancel (send /quit to exit)".into());
    lines.push("  Ctrl+D               Exit".into());
    lines.push(String::new());
    lines.push(format!(
        "{}",
        Style::new().bold().paint("Environment variables:")
    ));
    lines.push("  BRIOCHE_API_KEY      API key (can be passed via --api-key)".into());
    lines.push("  BRIOCHE_MODEL        LLM model (default: gpt-4o-mini)".into());
    lines.push("  BRIOCHE_BASE_URL     API endpoint (default: https://api.openai.com/v1)".into());
    lines.join("\n")
}
