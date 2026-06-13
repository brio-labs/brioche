//! Blocking REPL thread (reedline) and session manager — Book III §3.1.
//!
//! This module runs in `tokio::task::spawn_blocking`. It reads user
//! lines and forwards them via an `mpsc::Sender<String>` to an async
//! task that handles sending to the shell.
//!
//! Uses `reedline::ExternalPrinter` to allow the async bridge to
//! display messages without corrupting the reedline prompt.
//!
//! ## Invariants upheld
//! - I-Shell-Runtime-OnlyIO: All I/O is terminal-only; no Core state mutation.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;

use brioche_shell_runtime::BriocheShell;
use reedline::{
    Completer, DefaultHinter, DefaultPrompt, DefaultValidator, DescriptionMode, Emacs,
    ExternalPrinter, FileBackedHistory, IdeMenu, KeyCode, KeyModifiers, MenuBuilder, Reedline,
    ReedlineEvent, ReedlineMenu, Signal, Span, Suggestion, default_emacs_keybindings,
};
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// SessionManager (merged from session.rs)
// ---------------------------------------------------------------------------

/// Manages multiple CLI sessions.
///
/// Each session is identified by a unique ID and has its own
/// shell, history, and state.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct SessionManager {
    current: String,
    shells: BTreeMap<String, BriocheShell>,
}

impl SessionManager {
    /// Creates a new manager with an initial session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new(initial_id: impl Into<String>, initial_shell: BriocheShell) -> Self {
        let id = initial_id.into();
        let mut shells = BTreeMap::new();
        shells.insert(id.clone(), initial_shell);
        Self {
            current: id,
            shells,
        }
    }

    /// Reference to the current session's shell.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn current(&self) -> Option<&BriocheShell> {
        self.shells.get(&self.current)
    }

    /// Returns the current session ID.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn current_id(&self) -> &str {
        &self.current
    }

    /// Switches to another session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn switch(&mut self, id: &str) {
        if self.shells.contains_key(id) {
            self.current = id.to_string();
        }
    }

    /// Inserts a new session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn insert(&mut self, id: String, shell: BriocheShell) {
        self.shells.insert(id, shell);
    }

    /// Lists the IDs of all sessions in memory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn list(&self) -> Vec<&String> {
        self.shells.keys().collect()
    }

    /// Access to a session by its ID.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn get(&self, id: &str) -> Option<&BriocheShell> {
        self.shells.get(id)
    }
}

// ---------------------------------------------------------------------------
// REPL
// ---------------------------------------------------------------------------

/// Basic completer for Brioche terminal agents.
///
/// Completes slash commands (`/help`, `/quit`, `/session`…) and
/// file paths.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct BasicCompleter;

impl BasicCompleter {
    fn complete_slash(line: &str, pos: usize) -> Vec<Suggestion> {
        let commands = [
            ("/help", "Show help"),
            ("/quit", "Exit the CLI"),
            ("/session", "Show the current session"),
            ("/session new", "Create a new session"),
            ("/session list", "List sessions"),
            ("/session load", "Load a persisted session"),
        ];
        commands
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(line))
            .map(|(cmd, desc)| Suggestion {
                value: cmd.to_string(),
                description: Some(desc.to_string()),
                span: Span::new(0, pos),
                append_whitespace: true,
                ..Suggestion::default()
            })
            .collect()
    }

    fn complete_path(last_word: &str, pos: usize, word_start: usize) -> Vec<Suggestion> {
        let path = std::path::Path::new(last_word);
        let (dir, prefix) = if last_word.ends_with('/') {
            (path.to_path_buf(), "")
        } else {
            let fallback_dir = std::path::PathBuf::from(".");
            let dir = match path.parent().map(|p| p.to_path_buf()) {
                Some(p) => p,
                None => fallback_dir,
            };
            let prefix = path.file_name().and_then(|n| n.to_str()).map_or("", |s| s);
            (dir, prefix)
        };

        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };

        let mut suggestions = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) {
                let value = if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    format!("{name}/")
                } else {
                    name.to_string()
                };
                suggestions.push(Suggestion {
                    value,
                    span: Span::new(word_start, pos),
                    ..Suggestion::default()
                });
            }
        }
        suggestions
    }
}

impl Completer for BasicCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let prefix = &line[..pos];

        if prefix.starts_with('/') {
            return Self::complete_slash(prefix, pos);
        }

        let last_word_start = prefix.rfind(' ').map_or(0, |i| i + 1);
        let last_word = &prefix[last_word_start..];
        if last_word.starts_with('/') || last_word.starts_with('.') || last_word.starts_with('~') {
            return Self::complete_path(last_word, pos, last_word_start);
        }

        Vec::new()
    }
}

/// Launch the reedline loop and send each validated line over `input_tx`.
///
/// `/quit` and `/q` are handled directly by the REPL (immediate exit).
/// `Ctrl+C` and `Ctrl+D` also terminate the loop.
///
/// `printer` is passed to reedline to allow the bridge to print
/// messages without corrupting the prompt.
///
/// `cancel` is used to signal to the bridge and UI
/// that the program should terminate.
///
/// `completer` is optional — if provided, it will be used for
/// completion in the REPL. By default, `BasicCompleter` is used.
///
/// `history_path` is optional — if provided, reedline history
/// will be persisted to this file.
///
/// # Complexity
/// O(1) per iteration. Blocking on `reedline.read_line`.
///
/// # Panics
/// Never panics. All fallible operations use graceful fallbacks.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn run(
    input_tx: tokio::sync::mpsc::Sender<String>,
    printer: ExternalPrinter<String>,
    cancel: CancellationToken,
    completer: Option<Box<dyn reedline::Completer>>,
    history_path: Option<std::path::PathBuf>,
) {
    let history: Box<dyn reedline::History> = match history_path {
        Some(path) => match FileBackedHistory::with_file(1000, path) {
            Ok(h) => Box::new(h),
            Err(_) => {
                let fallback = FileBackedHistory::default();
                Box::new(match FileBackedHistory::new(1000) {
                    Ok(h) => h,
                    Err(_) => fallback,
                })
            }
        },
        None => {
            let fallback = FileBackedHistory::default();
            Box::new(match FileBackedHistory::new(1000) {
                Ok(h) => h,
                Err(_) => fallback,
            })
        }
    };

    let completer = match completer {
        Some(c) => c,
        None => Box::new(BasicCompleter),
    };

    let completion_menu = Box::new(
        IdeMenu::default()
            .with_name("completion_menu")
            .with_default_border()
            .with_description_mode(DescriptionMode::PreferRight)
            .with_min_description_width(20)
            .with_max_description_width(50),
    );

    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let mut reedline = Reedline::create()
        .with_history(history)
        .with_hinter(Box::new(DefaultHinter::default()))
        .with_validator(Box::new(DefaultValidator))
        .with_completer(completer)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(Box::new(Emacs::new(keybindings)))
        .with_external_printer(printer);

    let prompt = DefaultPrompt::default();

    loop {
        match reedline.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let trimmed = line.trim();
                // Immediate exit — does not go through the bridge.
                if trimmed == "/quit" || trimmed == "/q" {
                    tracing::info!("Goodbye.");
                    cancel.cancel();
                    break;
                }
                if trimmed.is_empty() {
                    continue;
                }
                if input_tx.blocking_send(line).is_err() {
                    cancel.cancel();
                    break;
                }
            }
            Ok(Signal::CtrlC) => {
                cancel.cancel();
                break;
            }
            Ok(Signal::CtrlD) => {
                cancel.cancel();
                break;
            }
            Ok(_) => continue,
            Err(err) => {
                tracing::warn!("Reedline error: {}", err);
                cancel.cancel();
                break;
            }
        }
    }
}
