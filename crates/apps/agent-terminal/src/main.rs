//! `agent-terminal` — Minimal terminal agent for Brioche.
//!
//! Minimal entry point: argument parsing, persistence initialization,
//! and dispatch to headless or interactive mode.
//!
//! Agent-specific modules:
//! - `shell_builder` — builds a complete `BriocheShell`
//! - `headless` — non-interactive mode (single command)
//! - `interactive` — REPL with multi-session support
//! - `bridge` — message routing and slash commands
//!
//! Shared terminal infrastructure (from `brioche-reedline`):
//! - `repl` — blocking read via reedline
//! - `session` — multi-session manager
//! - `ui` — terminal rendering
//!
//! Refs: SPECS.md §Book III-A, §Book III-C

use std::sync::Arc;

use brioche_shell_persistence::{RedbStorage, new_session_store};

mod bridge;
mod config;
mod headless;
mod interactive;
mod shell_builder;

use config::CliConfig;

/// agent-terminal — Minimal terminal agent for Brioche.
#[derive(argh::FromArgs, Debug)]
#[argh(
    name = "agent-terminal",
    description = "Minimal terminal agent for Brioche with LLM and system tools"
)]
struct Args {
    /// API key for the LLM provider (overrides BRIOCHE_API_KEY).
    #[argh(option, short = 'a', long = "api-key")]
    api_key: Option<String>,

    /// LLM model (overrides BRIOCHE_MODEL, default: gpt-4o-mini).
    #[argh(option, short = 'm', long = "model")]
    model: Option<String>,

    /// base URL for the API (overrides BRIOCHE_BASE_URL).
    #[argh(option, short = 'b', long = "base-url")]
    base_url: Option<String>,

    /// run a single prompt in non-interactive mode.
    #[argh(option, short = 'o', long = "one-shot")]
    one_shot: Option<String>,

    /// print version and exit.
    #[argh(switch, short = 'V', long = "version")]
    version: bool,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    let args: Args = argh::from_env();

    if args.version {
        println!("agent-terminal {VERSION}");
        std::process::exit(0);
    }

    let user_config = config::UserConfig {
        api_key: args.api_key,
        model: args.model,
        base_url: args.base_url,
    };
    let cli_config = CliConfig::from_env_and_args(user_config);

    if cli_config.openai.api_key.is_empty() {
        eprintln!(
            "{} No API key configured.",
            nu_ansi_term::Color::Yellow.paint("⚠")
        );
        eprintln!("   Use --api-key, the BRIOCHE_API_KEY env var, or see --help.");
        std::process::exit(1);
    }

    // Persistence (shared across all shells).
    let (redb_storage, session_store) = init_persistence();

    if let Some(prompt) = args.one_shot {
        headless::run(prompt, cli_config, redb_storage, session_store).await;
    } else {
        interactive::run(cli_config, redb_storage, session_store).await;
    }
}

/// Opens (or creates) the Redb database and returns the storage + store.
fn init_persistence() -> (RedbStorage, brioche_shell_persistence::SessionStore) {
    let data_dir = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".local/share/brioche"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/brioche"));
    if let Err(err) = std::fs::create_dir_all(&data_dir) {
        eprintln!("Failed to create data directory: {err}");
    }
    let db_path = data_dir.join("sessions.redb");

    let session_store = new_session_store();
    let redb_storage = match RedbStorage::new(&db_path, Arc::clone(&session_store)) {
        Ok(storage) => storage,
        Err(err) => {
            eprintln!("Failed to open Redb database: {err}. Using in-memory session only.");
            RedbStorage::new("/tmp/brioche-fallback.redb", Arc::clone(&session_store))
                .unwrap_or_else(|e| {
                    eprintln!("Fatal: cannot open fallback Redb: {e}");
                    std::process::exit(1);
                })
        }
    };

    (redb_storage, session_store)
}
