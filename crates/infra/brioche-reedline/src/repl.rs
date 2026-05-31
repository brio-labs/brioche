//! Thread REPL bloquant (reedline).
//!
//! Ce module tourne dans `tokio::task::spawn_blocking`. Il lit les
//! lignes utilisateur et les transmet via un `mpsc::Sender<String>`
//! à une task async qui s'occupe de l'envoi au shell.
//!
//! Utilise `reedline::ExternalPrinter` pour permettre au bridge async
//! d'afficher des messages sans corrompre le prompt reedline.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use reedline::{
    Completer, DefaultHinter, DefaultPrompt, DefaultValidator, ExternalPrinter, FileBackedHistory,
    Reedline, Signal, Span, Suggestion,
};
use tokio_util::sync::CancellationToken;

/// Compléteur de base pour les agents terminal Brioche.
///
/// Complète les commandes slash (`/help`, `/quit`, `/session`…) et
/// les chemins de fichiers.
pub struct BasicCompleter;

impl BasicCompleter {
    fn complete_slash(line: &str, pos: usize) -> Vec<Suggestion> {
        let commands = [
            ("/help", "Afficher l'aide"),
            ("/quit", "Quitter le CLI"),
            ("/session", "Afficher la session courante"),
            ("/session new", "Créer une nouvelle session"),
            ("/session list", "Lister les sessions"),
            ("/session load", "Charger une session persistée"),
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
            (
                path.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from(".")),
                path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
            )
        };

        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };

        let mut suggestions = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(prefix) {
                let value = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
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

        let last_word_start = prefix.rfind(' ').map(|i| i + 1).unwrap_or(0);
        let last_word = &prefix[last_word_start..];
        if last_word.starts_with('/') || last_word.starts_with('.') || last_word.starts_with('~') {
            return Self::complete_path(last_word, pos, last_word_start);
        }

        Vec::new()
    }
}

/// Lance la boucle reedline et envoie chaque ligne validée sur `input_tx`.
///
/// `/quit` et `/q` sont gérés directement par le REPL (sortie immédiate).
/// `Ctrl+C` et `Ctrl+D` terminent aussi la boucle.
///
/// `printer` est passé à reedline pour permettre au bridge d'imprimer
/// des messages sans corrompre le prompt.
///
/// `cancel` est utilisé pour signaler au bridge et à la UI
/// que le programme doit se terminer.
///
/// `completer` est optionnel — si fourni, il sera utilisé pour la
/// complétion dans le REPL. Par défaut, `BasicCompleter` est utilisé.
///
/// `history_path` est optionnel — si fourni, l'historique reedline
/// sera persisté dans ce fichier.
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
            Err(_) => Box::new(
                FileBackedHistory::new(1000).unwrap_or_else(|_| FileBackedHistory::default()),
            ),
        },
        None => {
            Box::new(FileBackedHistory::new(1000).unwrap_or_else(|_| FileBackedHistory::default()))
        }
    };

    let completer = completer.unwrap_or_else(|| Box::new(BasicCompleter));

    let mut reedline = Reedline::create()
        .with_history(history)
        .with_hinter(Box::new(DefaultHinter::default()))
        .with_validator(Box::new(DefaultValidator))
        .with_completer(completer)
        .with_external_printer(printer);

    let prompt = DefaultPrompt::default();

    loop {
        match reedline.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let trimmed = line.trim();
                // Sortie immédiate — ne passe pas par le bridge.
                if trimmed == "/quit" || trimmed == "/q" {
                    println!("Au revoir.");
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
                eprintln!("Erreur reedline: {err}");
                cancel.cancel();
                break;
            }
        }
    }
}
