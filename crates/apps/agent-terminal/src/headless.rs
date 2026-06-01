//! Mode headless : exécute un seul prompt et affiche le résultat sur stdout.
//!
//! Pas de REPL, pas de UI async, pas de `ExternalPrinter`.  On accumule
//! la réponse via le broadcast et on l'affiche à la fin.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::time::Duration;

use brioche_core::{ChatMessage, EngineInput};
use brioche_provider_openai::LlmChunk;
use brioche_shell_persistence::{RedbStorage, SessionStore};

use crate::config::CliConfig;
use crate::shell_builder::build_shell;

/// Exécute un seul prompt en mode non-interactif.
///
/// Accumule la réponse LLM pendant 30 s max, puis affiche le résultat
/// sur stdout et quitte avec le code 0.  En cas d'erreur réseau ou
/// LLM, affiche l'erreur sur stderr et quitte avec le code 1.
pub async fn run(
    prompt: String,
    cli_config: CliConfig,
    redb_storage: RedbStorage,
    session_store: SessionStore,
) {
    let (shell, llm_client, _llm_rx, _history) = build_shell(
        "headless",
        &cli_config,
        false, // pas de confirmation interactive en headless
        redb_storage,
        session_store,
        None,
        None,
    );

    llm_client
        .push_message(ChatMessage::User {
            content: prompt.clone(),
        })
        .await;

    if let Err(err) = shell.send_input(EngineInput::UserMessage(prompt)).await {
        eprintln!("Erreur d'envoi: {err}");
        std::process::exit(1);
    }

    // S'abonner au broadcast pour accumuler la réponse.
    let mut rx = llm_client.subscribe();
    let mut buffer = String::new();
    let mut done_received = false;
    let mut in_tool_call = false;

    loop {
        match tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
            Ok(Ok(chunk)) => match chunk {
                LlmChunk::Text(content) => {
                    buffer.push_str(&content);
                }
                LlmChunk::ToolCallStart { .. } => {
                    in_tool_call = true;
                    done_received = false;
                }
                LlmChunk::ToolCallDone { .. } => {
                    in_tool_call = false;
                }
                LlmChunk::ToolResult { name, output } => {
                    buffer.push_str(&format!("\n[tool {name}: {output}]\n"));
                }
                LlmChunk::Done => {
                    done_received = true;
                }
                LlmChunk::Error(error) => {
                    eprintln!("LLM error: {error}");
                    std::process::exit(1);
                }
                _ => {}
            },
            Ok(Err(_)) => break, // broadcast fermé
            Err(_) => {
                if done_received && !in_tool_call {
                    break;
                }
            }
        }
    }

    println!("{}", buffer.trim());
    std::process::exit(0);
}
