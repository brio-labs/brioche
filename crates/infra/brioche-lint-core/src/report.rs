//! Reporting utilities for Brioche linting tools.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, docs/SPECS.md §Book IV Ch 3 §3.5

use serde::Serialize;

/// Print `value` as pretty JSON to stdout.
///
/// Falls back to printing `[]` if serialization fails.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(_) => println!("[]"),
    }
}
