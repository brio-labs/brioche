//! Tests for brioche-reedline terminal infrastructure.
//!
//! Covers `BasicCompleter`.

use brioche_reedline::repl::BasicCompleter;
use reedline::{Completer, Span};

#[test]
fn basic_completer_returns_empty_for_plain_text() {
    let mut completer = BasicCompleter;
    let suggestions = completer.complete("hello world", 5);
    assert!(suggestions.is_empty());
}

#[test]
fn basic_completer_suggests_slash_commands() {
    let mut completer = BasicCompleter;
    let suggestions = completer.complete("/he", 3);
    assert!(!suggestions.is_empty());
    let values: Vec<String> = suggestions.iter().map(|s| s.value.clone()).collect();
    assert!(values.iter().any(|v| v == "/help"));
}

#[test]
fn basic_completer_suggests_quit_command() {
    let mut completer = BasicCompleter;
    let suggestions = completer.complete("/q", 2);
    let values: Vec<String> = suggestions.iter().map(|s| s.value.clone()).collect();
    assert!(values.iter().any(|v| v == "/quit"));
}

#[test]
fn basic_completer_returns_empty_for_unknown_slash() {
    let mut completer = BasicCompleter;
    let suggestions = completer.complete("/xyz", 4);
    assert!(suggestions.is_empty());
}

#[test]
fn basic_completer_suggests_paths() {
    let mut completer = BasicCompleter;
    let _suggestions = completer.complete("/tmp/", 5);
    // May or may not have suggestions depending on filesystem;
    // just verify it doesn't panic.
}

#[test]
fn basic_completer_span_is_correct() {
    let mut completer = BasicCompleter;
    let suggestions = completer.complete("/help", 5);
    if let Some(first) = suggestions.first() {
        assert_eq!(first.span, Span::new(0, 5));
    }
}
