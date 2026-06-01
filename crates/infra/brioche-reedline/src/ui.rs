//! Terminal rendering primitives for LLM output.
//!
//! Displays the LLM response **as a single block** when the stream
//! ends (no character-by-character streaming). This avoids reedline
//! redraw artifacts.
//!
//! Refs: I-Shell-Projection-Independent

use brioche_shell_runtime::LlmChunk;
use nu_ansi_term::{Color, Style};
use reedline::ExternalPrinter;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

fn render_markdown(text: &str) -> String {
    let mut result = text.to_string();
    result = render_delimited(&result, "`", "\x1b[36m", "\x1b[0m");
    result = render_delimited(&result, "**", "\x1b[1m", "\x1b[0m");
    result = render_delimited(&result, "*", "\x1b[3m", "\x1b[0m");
    result
}

fn render_delimited(text: &str, delim: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = text.split(delim).collect();
    if parts.len() < 2 {
        return text.to_string();
    }
    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            if i % 2 == 0 {
                result.push_str(close);
            } else {
                result.push_str(open);
            }
        }
        result.push_str(part);
    }
    if parts.len().is_multiple_of(2) {
        result.push_str(close);
    }
    result
}

fn render_block(text: &str) -> String {
    text.lines()
        .map(|line| {
            let mut r = render_markdown(line);
            if let Some(rest) = r.strip_prefix("# ") {
                r = format!("\x1b[1m\x1b[4m{rest}\x1b[0m");
            } else if let Some(rest) = r.strip_prefix("## ") {
                r = format!("\x1b[1m{rest}\x1b[0m");
            } else if let Some(rest) = r.strip_prefix("### ") {
                r = format!("\x1b[1m{rest}\x1b[0m");
            }
            if let Some(rest) = r.strip_prefix("- ") {
                r = format!("  • {rest}");
            }
            r
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Terminal render loop: receives LLM chunks via broadcast and
/// prints them to stdout when the stream ends.
///
/// Responses are rendered as a single block (no character-by-character
/// streaming) to avoid reedline redraw artifacts.
///
/// NOTE: We use `println!` instead of ` reedline::ExternalPrinter`
/// because `ExternalPrinter` only processes messages when the reedline
/// prompt is redrawn (i.e. on user input). While reedline blocks on
/// `read_line()`, messages sent via `ExternalPrinter` are invisible.
///
/// Refs: I-Shell-Projection-Independent
pub async fn run(
    mut llm_rx: broadcast::Receiver<LlmChunk>,
    cancel: CancellationToken,
    _printer: ExternalPrinter<String>,
) {
    let mut full_response = String::new();

    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => break,
            chunk = llm_rx.recv() => chunk,
        };

        match chunk {
            Ok(LlmChunk::Text(content)) => {
                full_response.push_str(&content);
            }
            Ok(LlmChunk::ToolCallStart { name, .. }) => {
                if !full_response.is_empty() {
                    println!("{}\n", render_block(&full_response));
                    full_response.clear();
                }
                println!(
                    "  {} Tool call: {}...\n",
                    Color::Cyan.paint("⚙"),
                    Style::new().bold().paint(name)
                );
            }
            Ok(LlmChunk::ToolCallDone { .. }) => {
                println!("  {}\n", Color::Cyan.paint("...done"));
            }
            Ok(LlmChunk::ToolResult { name, output }) => {
                let preview: String = output.lines().take(5).collect::<Vec<_>>().join("\n");
                let ellipsis = if output.lines().count() > 5 {
                    " ..."
                } else {
                    ""
                };
                println!(
                    "  {} Result from {}:\n    {}{}\n",
                    Color::Green.paint("✓"),
                    Style::new().bold().paint(name),
                    preview,
                    ellipsis
                );
            }
            Ok(LlmChunk::Done) if !full_response.is_empty() => {
                println!("{}\n", render_block(&full_response));
                full_response.clear();
            }
            Ok(LlmChunk::Error(error)) => {
                if !full_response.is_empty() {
                    println!("{}\n", render_block(&full_response));
                    full_response.clear();
                }
                let compact = error
                    .lines()
                    .find(|l| !l.trim().is_empty() && !l.trim().starts_with('{'))
                    .map(|l| l.trim().to_string())
                    .unwrap_or_else(|| error.lines().next().unwrap_or(&error).to_string());
                println!("  {} LLM error: {}\n", Color::Red.paint("✗"), compact);
            }
            Err(_) => break,
            _ => {}
        }
    }
}
