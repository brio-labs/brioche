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
use nu_ansi_term::Color;
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

/// Flush accumulated reasoning to the printer.
///
/// Called when transitioning away from reasoning (to content,
/// tool calls, or stream end). If `show` is false, the buffer
/// is silently discarded — reasoning is still preserved in the
/// history mirror, just not displayed.
fn flush_reasoning(printer: &ExternalPrinter<String>, buffer: &mut String, show: bool) {
    if !buffer.is_empty() && show {
        let text = std::mem::take(buffer);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let _ = printer.print(format!(
                "  {} {}",
                Color::Fixed(244).paint("💭"),
                Color::Fixed(244).paint(trimmed)
            ));
            wake_reedline();
        }
    } else {
        buffer.clear();
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

/// Fixed inner width for all boxes.
///
/// The terminal does not re-flow already-printed lines when resized.
/// If we adapt box width to the current terminal size, a box drawn
/// at 100 columns will break when the user shrinks to 50 columns.
///
/// A fixed conservative width (50 chars inside the borders) fits
/// comfortably in almost all terminals and stays intact on resize.
const BOX_WIDTH: usize = 50;

/// Wrap a line at a maximum display width, breaking on word boundaries
/// when possible. Returns the wrapped segments.
fn wrap_line(line: &str, max_width: usize) -> Vec<String> {
    if line.chars().count() <= max_width {
        return vec![line.to_string()];
    }

    let mut result = Vec::new();
    let mut current = String::new();

    for word in line.split_whitespace() {
        let word_len = word.chars().count();
        let current_len = current.chars().count();

        if word_len > max_width {
            // Word longer than max — flush current first, then break word
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            let mut w = word.to_string();
            while w.chars().count() > max_width {
                let (head, tail) = w.split_at(
                    w.char_indices()
                        .nth(max_width)
                        .map(|(i, _)| i)
                        .unwrap_or(w.len()),
                );
                result.push(head.to_string());
                w = tail.to_string();
            }
            current = w;
        } else if current_len + 1 + word_len > max_width {
            // Adding word would exceed width — flush and start new line
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            current = word.to_string();
        } else {
            // Append word
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        result.push(current);
    }
    result
}

/// Draw a unicode box around a label + content for the TUI.
///
/// Content is wrapped to fit within the terminal width so the right
/// border (`│`) does not break on narrow terminals.
///
/// ```text
/// ╭─ write_file ───────────╮
/// │  pending…              │
/// ╰────────────────────────╯
/// ```
fn box_lines(label: &str, content: &str) -> Vec<String> {
    let label_chars = label.chars().count();
    let content_max = content
        .lines()
        .flat_map(|l| wrap_line(l, BOX_WIDTH.saturating_sub(2)))
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let inner_w = (label_chars + 4)
        .max(content_max + 2)
        .max(28)
        .min(BOX_WIDTH);

    let mut lines = Vec::new();

    // Top border
    let mut top = String::from("╭─ ");
    top.push_str(label);
    top.push(' ');
    for _ in 0..inner_w.saturating_sub(label_chars + 3) {
        top.push('─');
    }
    top.push('╮');
    lines.push(top);

    // Content lines — wrap each input line and pad to inner_w
    for line in content.lines() {
        for wrapped in wrap_line(line, inner_w.saturating_sub(2)) {
            let line_chars = wrapped.chars().count();
            let pad = inner_w.saturating_sub(line_chars + 2);
            let mut l = format!("│ {}", wrapped);
            for _ in 0..pad {
                l.push(' ');
            }
            l.push_str(" │");
            lines.push(l);
        }
    }

    // Bottom border
    let mut bottom = String::from("╰");
    for _ in 0..inner_w {
        bottom.push('─');
    }
    bottom.push('╯');
    lines.push(bottom);

    lines
}

/// Draw a compact error block with optional suggestion.
///
/// Lines are wrapped to fit the terminal so borders stay intact.
fn error_lines(
    code: &str,
    message: &str,
    source: &str,
    recoverable: bool,
    suggestion: Option<&str>,
) -> Vec<String> {
    let severity = if recoverable { "ERROR" } else { "FATAL" };

    let header = format!("{}: {}", code, message);
    let wrapped_header = wrap_line(&header, BOX_WIDTH.saturating_sub(2));
    let wrapped_suggestion =
        suggestion.map(|h| wrap_line(&format!("→ {}", h), BOX_WIDTH.saturating_sub(4)));

    // Compute inner width from actual wrapped content
    let content_max = wrapped_header
        .iter()
        .chain(wrapped_suggestion.iter().flatten())
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0);
    let top_label_len = severity.chars().count() + source.chars().count() + 7;
    let inner_w = top_label_len.max(content_max + 2).max(28).min(BOX_WIDTH);

    let mut lines = Vec::new();

    // Top border
    let mut top = String::from("┌─ ");
    top.push_str(severity);
    top.push_str(" ─ [");
    top.push_str(source);
    top.push_str("] ");
    for _ in 0..inner_w.saturating_sub(top_label_len) {
        top.push('─');
    }
    top.push('┐');
    lines.push(top);

    // Content
    for line in &wrapped_header {
        let pad = inner_w.saturating_sub(line.chars().count() + 2);
        let mut l = format!("│ {}", line);
        for _ in 0..pad {
            l.push(' ');
        }
        l.push_str(" │");
        lines.push(l);
    }

    if let Some(sugg_lines) = wrapped_suggestion {
        lines.push("│".to_string());
        for line in sugg_lines {
            let pad = inner_w.saturating_sub(line.chars().count() + 2);
            let mut l = format!("│   {}", line);
            for _ in 0..pad {
                l.push(' ');
            }
            l.push_str(" │");
            lines.push(l);
        }
    }

    // Bottom border
    let mut bottom = String::from("└");
    for _ in 0..inner_w {
        bottom.push('─');
    }
    bottom.push('┘');
    lines.push(bottom);

    lines
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
    let mut reasoning_buffer = String::new();

    // When true, reasoning is displayed as it accumulates.
    // When false (default), reasoning is silently collected for
    // history continuity but not shown in the UI.
    let show_reasoning = std::env::var("BRIOCHE_SHOW_REASONING")
        .is_ok_and(|s| s == "1" || s.eq_ignore_ascii_case("true"));

    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => break,
            chunk = llm_rx.recv() => chunk,
        };

        match chunk {
            Ok(ShellEvent::LlmText(content)) => {
                flush_reasoning(&printer, &mut reasoning_buffer, show_reasoning);
                full_response.push_str(&content);
            }
            Ok(ShellEvent::LlmReasoning(content)) => {
                reasoning_buffer.push_str(&content);
            }
            Ok(ShellEvent::LlmToolCallStart { name, .. }) => {
                flush_reasoning(&printer, &mut reasoning_buffer, show_reasoning);
                if !full_response.is_empty() {
                    let _ = printer.print(render_block(&full_response));
                    wake_reedline();
                    full_response.clear();
                }
                for line in box_lines(&name, "pending…") {
                    let _ = printer.print(line);
                }
                wake_reedline();
            }
            Ok(ShellEvent::LlmToolCallDone { .. }) => {
                let _ = printer.print("  … done".to_string());
                wake_reedline();
            }
            Ok(ShellEvent::ToolResult { name, output }) => {
                let trimmed = output.trim();
                let preview = if trimmed.lines().count() > 10 {
                    trimmed.lines().take(10).collect::<Vec<_>>().join("\n") + "\n… (truncated)"
                } else {
                    trimmed.to_string()
                };
                for line in box_lines(&format!("Result: {}", name), &preview) {
                    let _ = printer.print(line);
                }
                wake_reedline();
            }
            Ok(ShellEvent::LlmDone) => {
                flush_reasoning(&printer, &mut reasoning_buffer, show_reasoning);
                if !full_response.is_empty() {
                    let _ = printer.print(render_block(&full_response));
                    wake_reedline();
                    full_response.clear();
                }
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
                for line in error_lines(&code, &message, source, recoverable, suggestion.as_deref())
                {
                    let _ = printer.print(line);
                }
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
