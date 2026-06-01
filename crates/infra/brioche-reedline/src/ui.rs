//! Terminal rendering primitives for LLM output.
//!
//! Displays the LLM response **as a single block** when the stream
//! ends (no character-by-character streaming). This avoids reedline
//! redraw artifacts.
//!
//! Uses `reedline::ExternalPrinter` so output appears above the
//! prompt without corrupting reedline's cursor state. After each
//! message we send `SIGWINCH` to force reedline to wake from its
//! blocking `crossterm::event::read()` and repaint, which processes
//! the `ExternalPrinter` queue immediately.
//!
//! Refs: I-Shell-Projection-Independent

use brioche_shell_runtime::ShellEvent;
use nu_ansi_term::{Color, Style};
use reedline::ExternalPrinter;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Wake reedline from its blocking `read_line()` so it repaints
/// and processes the `ExternalPrinter` queue immediately.
///
/// On Unix we send `SIGWINCH` to our own process. crossterm's
/// signal handler converts this into an `Event::Resize`, which
/// reedline handles by repainting the prompt — and during that
/// repaint it prints any queued external messages.
///
/// On Windows this is a no-op; messages appear on the next
/// keypress, which is acceptable.
fn wake_reedline() {
    #[cfg(unix)]
    unsafe {
        libc::kill(libc::getpid(), libc::SIGWINCH);
    }
}

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
/// displays them via `ExternalPrinter`.
///
/// After each message we send `SIGWINCH` to force reedline to
/// wake from its blocking read and repaint, processing the
/// external printer queue immediately.
///
/// Refs: I-Shell-Projection-Independent
pub async fn run(
    mut llm_rx: broadcast::Receiver<ShellEvent>,
    cancel: CancellationToken,
    printer: ExternalPrinter<String>,
) {
    let mut full_response = String::new();

    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => break,
            chunk = llm_rx.recv() => chunk,
        };

        match chunk {
            Ok(ShellEvent::LlmText(content)) => {
                full_response.push_str(&content);
            }
            Ok(ShellEvent::LlmToolCallStart { name, .. }) => {
                if !full_response.is_empty() {
                    let _ = printer.print(render_block(&full_response));
                    wake_reedline();
                    full_response.clear();
                }
                let _ = printer.print(format!(
                    "  {} Tool call: {}...",
                    Color::Cyan.paint("⚙"),
                    Style::new().bold().paint(name)
                ));
                wake_reedline();
            }
            Ok(ShellEvent::LlmToolCallDone { .. }) => {
                let _ = printer.print(format!("  {}", Color::Cyan.paint("...done")));
                wake_reedline();
            }
            Ok(ShellEvent::ToolResult { name, output }) => {
                let preview: String = output.lines().take(5).collect::<Vec<_>>().join("\n");
                let ellipsis = if output.lines().count() > 5 {
                    " ..."
                } else {
                    ""
                };
                let _ = printer.print(format!(
                    "  {} Result from {}:\n    {}{}",
                    Color::Green.paint("✓"),
                    Style::new().bold().paint(name),
                    preview,
                    ellipsis
                ));
                wake_reedline();
            }
            Ok(ShellEvent::LlmDone) if !full_response.is_empty() => {
                let _ = printer.print(render_block(&full_response));
                wake_reedline();
                full_response.clear();
            }
            Ok(ShellEvent::Error {
                code,
                message,
                source,
                recoverable,
                suggestion,
            }) => {
                if !full_response.is_empty() {
                    let _ = printer.print(render_block(&full_response));
                    wake_reedline();
                    full_response.clear();
                }
                let severity = if recoverable { "ERROR" } else { "FATAL" };
                let mut text = format!(
                    "  {} [{}][{}] {}: {}",
                    Color::Red.paint("✗"),
                    severity,
                    source,
                    code,
                    message.lines().next().unwrap_or(&message)
                );
                if let Some(hint) = suggestion {
                    text.push_str(&format!("\n     → {}", hint));
                }
                let _ = printer.print(text);
                wake_reedline();
            }
            Ok(ShellEvent::Warning { message, source }) => {
                let _ = printer.print(format!(
                    "  {} [{}] {}",
                    Color::Yellow.paint("⚠"),
                    source,
                    message
                ));
                wake_reedline();
            }
            Ok(ShellEvent::Status { message }) => {
                let _ = printer.print(format!("  {} {}", Color::Blue.paint("ℹ"), message));
                wake_reedline();
            }
            Ok(ShellEvent::Thinking { message }) => {
                let _ = printer.print(format!("  {} {}", Color::Blue.paint("◐"), message));
                wake_reedline();
            }
            Err(_) => break,
            _ => {}
        }
    }
}
