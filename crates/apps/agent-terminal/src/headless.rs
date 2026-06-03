//! Headless mode: runs a single prompt and prints the result to stdout.
//!
//! No REPL, no async UI, no `ExternalPrinter`. We accumulate the
//! response via broadcast and print it at the end.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::time::Duration;

use brioche_core::{ChatMessage, EngineInput};
use brioche_provider_openai::ShellEvent;
use brioche_shell_persistence::{RedbStorage, SessionStore};

use crate::config::CliConfig;
use crate::shell_builder::build_shell;

/// Draw a box around a label + content for headless output.
///
/// ```text
/// ╭─ read_file ───────────╮
/// │  path: "/etc/passwd"  │
/// ╰───────────────────────╯
/// ```
fn draw_box(label: &str, content: &str) {
    let label_chars = label.chars().count();
    let content_max = content
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let inner_w = (label_chars + 4).max(content_max + 2).max(28);

    // Top border: ╭─ label ────────╮
    let mut top = String::from("╭─ ");
    top.push_str(label);
    top.push(' ');
    for _ in 0..inner_w.saturating_sub(label_chars + 3) {
        top.push('─');
    }
    top.push('╮');
    eprintln!("{}", top);

    // Content lines
    for line in content.lines() {
        let line_chars = line.chars().count();
        let pad = inner_w.saturating_sub(line_chars + 2);
        eprint!("│ {}", line);
        for _ in 0..pad {
            eprint!(" ");
        }
        eprintln!(" │");
    }

    // Bottom border
    eprint!("╰");
    for _ in 0..inner_w {
        eprint!("─");
    }
    eprintln!("╯");
}

/// Draw a compact error block with optional suggestion.
fn draw_error(
    code: &str,
    message: &str,
    source: &str,
    recoverable: bool,
    suggestion: Option<&str>,
) {
    let severity = if recoverable { "ERROR" } else { "FATAL" };
    eprintln!("┌─ {} ─ [{}] ───────────────┐", severity, source);
    let header = format!("{}: {}", code, message);
    for line in header.lines() {
        eprintln!("│ {}", line);
    }
    if let Some(hint) = suggestion {
        eprintln!("│");
        eprintln!("│  → {}", hint);
    }
    eprintln!("└─────────────────────────────────┘");
}

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
    )
    .await;

    llm_client
        .push_message(ChatMessage::User {
            content: prompt.clone(),
        })
        .await;

    if let Err(err) = shell.send_input(EngineInput::UserMessage(prompt)).await {
        draw_error(
            "SendFailed",
            &err.to_string(),
            "headless",
            false,
            Some("The engine disconnected."),
        );
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
                    ShellEvent::LlmText(content) => {
                        flush_reasoning(&mut reasoning_buffer, show_reasoning);
                        buffer.push_str(&content);
                        done_received = false;
                    }
                    ShellEvent::LlmReasoning(content) => {
                        reasoning_buffer.push_str(&content);
                    }
                    ShellEvent::LlmToolCallStart { name, .. } => {
                        flush_reasoning(&mut reasoning_buffer, show_reasoning);
                        draw_box(&name, "pending…");
                        in_tool_call = true;
                        done_received = false;
                    }
                    ShellEvent::LlmToolCallDone { .. } => {
                        eprintln!("  … done");
                        in_tool_call = false;
                    }
                    ShellEvent::ToolResult { name, output } => {
                        let trimmed = output.trim();
                        let preview = if trimmed.lines().count() > 10 {
                            trimmed.lines().take(10).collect::<Vec<_>>().join("\n")
                                + "\n… (truncated)"
                        } else {
                            trimmed.to_string()
                        };
                        draw_box(&format!("Result: {name}"), &preview);
                    }
                    ShellEvent::LlmDone => {
                        flush_reasoning(&mut reasoning_buffer, show_reasoning);
                        done_received = true;
                    }
                    ShellEvent::Error {
                        code,
                        message,
                        source,
                        recoverable,
                        suggestion,
                    } => {
                        draw_error(&code, &message, source, recoverable, suggestion.as_deref());
                        if !recoverable {
                            std::process::exit(1);
                        }
                    }
                    ShellEvent::Warning { message, source } => {
                        eprintln!("⚠  [{}] {}", source, message);
                    }
                    ShellEvent::Status { message } => {
                        eprintln!("◐  {}", message);
                    }
                    ShellEvent::Thinking { message } => {
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
