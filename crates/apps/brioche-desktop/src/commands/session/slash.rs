//! Slash-command parsing and dispatch.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use tauri::{AppHandle, Emitter};

use super::chat::{emit_system, rebuild_current_session};
use crate::state::DesktopState;

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
pub(super) async fn handle_slash_command(
    app: &AppHandle,
    state: &DesktopState,
    cmd: &str,
    full_line: &str,
) -> Result<(), String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    match parts.first().copied() {
        Some("help") | Some("h") => {
            emit_system(app, print_help());
            Ok(())
        }
        Some("quit") | Some("q") => {
            let _ = app.emit("app-exit", ());
            Ok(())
        }
        Some("clear") | Some("c") => {
            rebuild_current_session(state).await?;
            emit_system(app, "History cleared.");
            Ok(())
        }
        Some("session") => handle_session_subcommand(app, state, &parts[1..]).await,
        _ => {
            emit_system(app, format!("Unknown command: {full_line}"));
            Ok(())
        }
    }
}

async fn handle_session_subcommand(
    app: &AppHandle,
    state: &DesktopState,
    args: &[&str],
) -> Result<(), String> {
    let Some(command) = args.first().copied() else {
        emit_system(app, "/session requires a sub-command");
        return Ok(());
    };

    match command {
        "new" => {
            let new_id = super::lifecycle::new_session_impl(state).await?;
            emit_system(app, format!("New session: {new_id}"));
        }
        "list" | "" => {
            state.ensure_manager().await?;
            let mgr = state.manager.read().await;
            if let Some(manager) = mgr.as_ref() {
                emit_system(app, super::listing::session_lines(manager).join("\n"));
            }
        }
        "load" => {
            let Some(id) = args.get(1).copied() else {
                emit_system(app, "/session load requires a session id");
                return Ok(());
            };
            super::loading::load_session(app, state, id).await?;
        }
        other => emit_system(app, format!("Unknown /session {} command", other)),
    }
    Ok(())
}

/// Help text for slash commands.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub(super) fn print_help() -> String {
    [
        "Commands:",
        "  <text>               Send a message to the LLM",
        "  /help                Show this help",
        "  /quit                Exit the app",
        "  /clear               Clear conversation history",
        "  /session             Show current session",
        "  /session new         Create a new session",
        "  /session list        List sessions",
        "  /session load <id>   Load a persisted session",
        "",
        "Environment variables:",
        "  BRIOCHE_API_KEY      API key",
        "  BRIOCHE_MODEL        LLM model (default: gpt-4o-mini)",
        "  BRIOCHE_BASE_URL     API endpoint",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_help_contains_commands() {
        let help = print_help();
        assert!(help.contains("/help"), "expected /help in help text");
        assert!(help.contains("/session"), "expected /session in help text");
    }
}
