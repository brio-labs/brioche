//! Headless mode: runs a single prompt and prints the result to stdout.
//!
//! No REPL, no async UI, no `ExternalPrinter`. We accumulate the
//! response via broadcast and print it at the end.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::time::Duration;

use brioche_core::{ChatMessage, EngineInput};
use brioche_provider_openai::LlmChunk;
use brioche_shell_persistence::{RedbStorage, SessionStore};

use crate::config::CliConfig;
use crate::shell_builder::build_shell;

/// Runs a single prompt in non-interactive mode.
///
/// Accumulates the LLM response for up to 30 s, then prints the result
/// to stdout and exits with code 0. On network or LLM error, prints
/// the error to stderr and exits with code 1.
pub async fn run(
    prompt: String,
    cli_config: CliConfig,
    redb_storage: RedbStorage,
    session_store: SessionStore,
) {
    let (shell, llm_client, _llm_rx, _history) = build_shell(
        "headless",
        &cli_config,
        false, // no interactive confirmation in headless mode
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
        eprintln!("Send error: {err}");
        std::process::exit(1);
    }

    // Subscribe to the broadcast to accumulate the response.
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
            Ok(Err(_)) => break, // broadcast closed
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
