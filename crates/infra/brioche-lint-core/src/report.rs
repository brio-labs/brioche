//! Reporting utilities for Brioche linting tools.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, docs/SPECS.md §Book IV Ch 3 §3.5

use serde::Serialize;

/// Format `value` as pretty JSON.
///
/// Falls back to `"[]"` if serialization fails.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn format_json<T: Serialize>(value: &T) -> String {
    match serde_json::to_string_pretty(value) {
        Ok(json) => json,
        Err(_) => "[]".to_string(),
    }
}
