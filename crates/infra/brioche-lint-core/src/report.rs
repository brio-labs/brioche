//! Reporting helpers for Brioche lint tooling.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, §3.5

use std::path::Path;

/// Format a violation located at a specific file and line.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
pub fn format_file_violation(path: &Path, line: usize, message: &str) -> String {
    format!("  {}:{} — {}", path.display(), line, message)
}

/// Format a summary header when violations were found.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
pub fn format_violation_header(count: usize) -> String {
    format!("Found {} violation(s):", count)
}

/// Format a success message.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn format_success(message: &str) -> String {
    format!("{} ✓", message)
}

/// Format a list of violations as bullet points.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn format_bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("  - {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}
