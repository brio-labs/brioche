//! `brioche-cli` — Shell Terminal pour Brioche.
//!
//! Point d'entrée minimal : parsing des arguments, initialisation de
//! la persistence, et dispatch vers le mode headless ou interactif.
//!
//! Toute la logique métier vit dans les modules fils :
//! - `shell_builder` — construction d'un `BriocheShell` complet
//! - `headless` — mode non-interactif (une seule commande)
//! - `interactive` — mode REPL avec multi-session
//! - `bridge` — routing des messages et commandes slash
//! - `repl` — lecture bloquante via reedline
//! - `ui` — rendu terminal
//!
//! Refs: SPECS.md §Book III-A, §Book III-C

use std::sync::Arc;

use brioche_shell_persistence::{RedbStorage, new_session_store};
use clap::Parser;

mod bridge;
mod config;
mod headless;
mod interactive;
mod repl;
mod session_manager;
mod shell_builder;
mod ui;

use config::CliConfig;

/// Brioche CLI — Shell Terminal avec LLM et outils système.
#[derive(Parser, Debug)]
#[command(name = "brioche-cli")]
#[command(about = "Interactive shell terminal for Brioche with LLM and system tools")]
#[command(version)]
struct Args {
    /// Clé API pour le provider LLM (override BRIOCHE_API_KEY).
    #[arg(short, long, env = "BRIOCHE_API_KEY")]
    api_key: Option<String>,

    /// Modèle LLM (override BRIOCHE_MODEL, défaut: gpt-4o-mini).
    #[arg(short, long, env = "BRIOCHE_MODEL")]
    model: Option<String>,

    /// URL de base de l'API (override BRIOCHE_BASE_URL).
    #[arg(short, long, env = "BRIOCHE_BASE_URL")]
    base_url: Option<String>,

    /// Exécuter un seul prompt en mode non-interactif.
    #[arg(short, long)]
    one_shot: Option<String>,

    /// Désactiver la confirmation interactive pour les commandes shell.
    #[arg(long)]
    no_confirm: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let user_config = config::UserConfig {
        api_key: args.api_key,
        model: args.model,
        base_url: args.base_url,
    };
    let cli_config = CliConfig::from_env_and_args(user_config);

    if cli_config.openai.api_key.is_empty() {
        eprintln!(
            "{} Aucune clé API configurée.",
            nu_ansi_term::Color::Yellow.paint("⚠")
        );
        eprintln!("   Utilisez --api-key, la variable BRIOCHE_API_KEY, ou voyez --help.");
        std::process::exit(1);
    }

    // Persistence (partagée entre tous les shells).
    let (redb_storage, session_store) = init_persistence();

    if let Some(prompt) = args.one_shot {
        headless::run(prompt, cli_config, redb_storage, session_store).await;
    } else {
        interactive::run(cli_config, redb_storage, session_store, !args.no_confirm).await;
    }
}

/// Ouvre (ou crée) la base Redb et retourne le stockage + le store.
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
