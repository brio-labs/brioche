//! Headless mode: execute a single prompt and print the result to stdout.
//!
//! No REPL, no async UI, no `ExternalPrinter`. Accumulates the
//! response via the broadcast and prints at the end.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::time::Duration;

use brioche_core::{ChatMessage, EngineInput};
use brioche_provider_openai::LlmChunk;
use brioche_shell_persistence::{RedbStorage, SessionStore};

use crate::config::CliConfig;
use crate::shell_builder::build_shell;

/// Print accumulated reasoning to stderr, or silently discard it.
fn flush_reasoning(buffer: &mut String, show: bool) {
    if !buffer.is_empty() && show {
        let text = std::mem::take(buffer);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            eprintln!("💭  {}", trimmed);
        }
    } else {
        buffer.clear();
    }
}

/// Runs a single prompt in non-interactive mode.
///
/// Accumulates the LLM response for up to 30 s, then prints the result
/// to stdout and exits with code 0. On network or LLM error, prints
/// the error to stderr and exits with code 1.
///
/// # Cancel safety
/// This future holds only local state across await points. Dropping it
/// stops the prompt and exits the process without cleanup.
pub async fn run(
    prompt: String,
    cli_config: CliConfig,
    redb_storage: RedbStorage,
    session_store: SessionStore,
) {
    let (shell, llm_client, _llm_rx, _history) = build_shell(
        "headless",
        &cli_config,
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
    let mut reasoning_buffer = String::new();
    let mut done_received = false;
    let mut in_tool_call = false;
    let mut consecutive_timeouts = 0u32;

    let show_reasoning = std::env::var("BRIOCHE_SHOW_REASONING")
        .is_ok_and(|s| s == "1" || s.eq_ignore_ascii_case("true"));

    loop {
        match tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
            Ok(Ok(chunk)) => {
                consecutive_timeouts = 0;
                match chunk {
                    LlmChunk::Text(content) => {
                        flush_reasoning(&mut reasoning_buffer, show_reasoning);
                        buffer.push_str(&content);
                        done_received = false;
                    }
                    LlmChunk::Reasoning(content) => {
                        reasoning_buffer.push_str(&content);
                    }
                    LlmChunk::ToolCallStart { .. } => {
                        flush_reasoning(&mut reasoning_buffer, show_reasoning);
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
                        flush_reasoning(&mut reasoning_buffer, show_reasoning);
                        done_received = true;
                    }
                    LlmChunk::Error(error) => {
                        eprintln!("LLM error: {error}");
                        std::process::exit(1);
                    }
                    LlmChunk::Warning(message) => {
                        eprintln!("⚠  {}", message);
                    }
                    LlmChunk::Status(message) => {
                        eprintln!("◐  {}", message);
                    }
                    _ => {}
                }
            }
            Ok(Err(_)) => break, // broadcast closed
            Err(_) => {
                consecutive_timeouts += 1;
                if consecutive_timeouts == 1 {
                    eprintln!(
                        "\n⚠  No response after 30s — model may be reasoning slowly or provider is queuing…"
                    );
                } else if consecutive_timeouts >= 3 {
                    eprintln!(
                        "\n✗  Model unresponsive after {}s. Aborting.",
                        consecutive_timeouts * 30
                    );
                    std::process::exit(1);
                }
                if done_received && !in_tool_call {
                    break;
                }
            }
        }
    }

    println!("{}", buffer.trim());
    std::process::exit(0);
}
