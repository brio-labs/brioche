//! Interactive mode: REPL with multi-session and terminal rendering.
//!
//! Assembles three worker threads:
//! 1. REPL (blocking, reedline) → sends lines over a channel.
//! 2. Bridge (async) → receives lines, routes to the shell.
//! 3. UI (async) → receives LLM chunks, displays via ExternalPrinter.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_shell_persistence::{RedbStorage, SessionStore};
use nu_ansi_term::Color;
use reedline::ExternalPrinter;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::bridge;
use crate::config::CliConfig;
use crate::session_manager::SessionManager;
use crate::shell_builder::build_shell;

/// Launches the full interactive mode.
pub async fn run(
    cli_config: CliConfig,
    redb_storage: RedbStorage,
    session_store: SessionStore,
    with_confirm: bool,
) {
    print_banner();

    let (shell, llm_client, llm_rx, _history) = build_shell(
        "cli-session",
        &cli_config,
        with_confirm,
        redb_storage.clone(),
        Arc::clone(&session_store),
        None,
        None,
    );

    let manager = Arc::new(RwLock::new(SessionManager::new("cli-session", shell)));

    let (input_tx, input_rx) = mpsc::channel::<String>(64);
    let cancel = CancellationToken::new();
    let printer = ExternalPrinter::<String>::default();

    let factory = bridge::ShellFactory {
        redb: redb_storage,
        store: session_store,
        config: cli_config,
        with_confirm,
    };

    // Clone for the bridge (the bridge takes ownership of its clones).
    let llm_for_bridge = llm_client.clone();
    let bridge_handle = tokio::spawn(bridge::run(
        input_rx,
        cancel.clone(),
        printer.clone(),
        Arc::clone(&manager),
        llm_for_bridge,
        factory,
    ));

    // Drop the original — when the bridge finishes and drops its copy,
    // the broadcast channel closes and the UI exits cleanly.
    drop(llm_client);

    let printer_for_ui = printer.clone();
    let cancel_for_ui = cancel.clone();
    let ui_handle = tokio::spawn(crate::ui::run(llm_rx, cancel_for_ui, printer_for_ui));

    let cancel_for_repl = cancel.clone();
    let repl_handle =
        tokio::task::spawn_blocking(move || crate::repl::run(input_tx, printer, cancel_for_repl));

    let (bridge_res, ui_res, repl_res) = tokio::join!(bridge_handle, ui_handle, repl_handle);
    if let Err(e) = bridge_res {
        eprintln!("bridge task panicked or was cancelled: {e}");
    }
    if let Err(e) = ui_res {
        eprintln!("ui task panicked or was cancelled: {e}");
    }
    if let Err(e) = repl_res {
        eprintln!("repl task panicked or was cancelled: {e}");
    }
}

fn print_banner() {
    println!(
        "{}",
        Color::Cyan.paint("╔═══════════════════════════════════════╗")
    );
    println!(
        "{}",
        Color::Cyan.paint("║      Brioche CLI — Shell Terminal     ║")
    );
    println!(
        "{}",
        Color::Cyan.paint("╚═══════════════════════════════════════╝")
    );
    println!();
    println!(
        "{} Type /help for the list of commands.",
        Color::Green.paint("→")
    );
    println!();
}
