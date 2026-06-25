//! Reporting helpers for Brioche lint tooling.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, §3.5

use std::path::Path;

/// Print a violation located at a specific file and line.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
pub fn print_file_violation(path: &Path, line: usize, message: &str) {
    println!("  {}:{} — {}", path.display(), line, message);
}

/// Print a summary header when violations were found.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
pub fn print_violation_header(count: usize) {
    println!("Found {} violation(s):\n", count);
}

/// Print a success message.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn print_success(message: &str) {
    println!("{} ✓", message);
}

/// Print a list of violations as bullet points.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn print_bullet_list(items: &[String]) {
    for item in items {
        println!("  - {item}");
    }
}
