//! `cargo-brioche-lint` — plugin linter — Book V.
//!
//! Detects forbidden patterns in Brioche plugin code:
//! - Direct `session.history` or `session.state` field access.
//! - `HashMap` / `HashSet` in persisted state.
//! - `unwrap()` / `expect()` in plugin hooks.
//!
//! ## Usage
//! ```text
//! cargo brioche-lint --path crates/my-plugin
//! ```
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.5

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use walkdir::WalkDir;

/// CLI arguments.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
#[derive(Parser)]
#[command(name = "cargo-brioche-lint")]
#[command(about = "Lint Brioche plugins for forbidden patterns")]
struct Cli {
    /// Path to the plugin crate.
    #[arg(long, short, default_value = ".")]
    path: PathBuf,
}

/// A single lint violation.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
#[derive(Debug)]
struct Violation {
    file: String,
    line: usize,
    message: String,
}

/// Entry point.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
fn main() {
    let cli = Cli::parse();
    let violations = lint_directory(&cli.path);

    if violations.is_empty() {
        println!("No violations found ✓");
        std::process::exit(0);
    }

    println!("Found {} violation(s):\n", violations.len());
    for v in &violations {
        println!("  {}:{} — {}", v.file, v.line, v.message);
    }
    std::process::exit(1);
}

/// Scan a directory for lint violations.
///
/// # Complexity
/// O(n · m) where n = files scanned, m = lines per file.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
fn lint_directory(root: &PathBuf) -> Vec<Violation> {
    let mut violations = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.extension().is_some_and(|ext| ext == "rs")
                && !p.components().any(|c| {
                    let s = c.as_os_str().to_string_lossy();
                    s == "target" || s == ".git"
                })
        })
    {
        let path = entry.path();
        let contents = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        lint_file_contents(path, &contents, &mut violations);
    }

    violations
}

/// Lint a single file's contents.
///
/// # Complexity
/// O(m) where m = lines in file.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
fn lint_file_contents(path: &std::path::Path, contents: &str, violations: &mut Vec<Violation>) {
    let file = path.display().to_string();
    let has_extension_type = contents.contains("BriocheExtensionType");

    // Pattern 1: direct session.history or session.state access.
    let forbidden_fields = ["session.history", "session.state"];
    for (line_no, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }

        for field in &forbidden_fields {
            if line.contains(field) {
                violations.push(Violation {
                    file: file.clone(),
                    line: line_no + 1,
                    message: format!(
                        "Direct `{field}` access in plugin code. Use `ExtensionStorage` and `SessionSnapshot` instead."
                    ),
                });
            }
        }

        // Pattern 2: unwrap / expect in plugin code.
        if line.contains(".unwrap()") || line.contains(".expect(") {
            violations.push(Violation {
                file: file.clone(),
                line: line_no + 1,
                message: "Found unwrap/expect. Use explicit error handling instead.".into(),
            });
        }

        // Pattern 3: HashMap / HashSet in BriocheExtensionType persisted state.
        if has_extension_type && (line.contains("HashMap") || line.contains("HashSet")) {
            violations.push(Violation {
                file: file.clone(),
                line: line_no + 1,
                message: "HashMap/HashSet in BriocheExtensionType state. Use BTreeMap/BTreeSet for determinism.".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_session_history_access() {
        let contents = "fn f() { let _ = session.history; }";
        let mut violations = Vec::new();
        lint_file_contents(std::path::Path::new("test.rs"), contents, &mut violations);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("session.history"));
    }

    #[test]
    fn detects_unwrap_and_expect() {
        let contents = "fn f() {\n    let x = y.unwrap();\n    let z = w.expect(\"ok\");\n}";
        let mut violations = Vec::new();
        lint_file_contents(std::path::Path::new("test.rs"), contents, &mut violations);
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn detects_hashmap_in_extension_state() {
        let contents = r#"
/// Test state with disordered collection.
///
/// # Invariants
/// - I-Eco-OrderedCollections
///
/// Snapshot: FullClone (< 256 bytes).
#[derive(BriocheExtensionType)]
pub struct BadState {
    pub data: HashMap<String, u64>,
}
"#;
        let mut violations = Vec::new();
        lint_file_contents(std::path::Path::new("test.rs"), contents, &mut violations);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("HashMap"));
    }

    #[test]
    fn ignores_hashmap_without_extension_type() {
        let contents = "fn f() { let _: HashMap<String, u64> = HashMap::new(); }";
        let mut violations = Vec::new();
        lint_file_contents(std::path::Path::new("test.rs"), contents, &mut violations);
        assert!(violations.is_empty());
    }

    #[test]
    fn ignores_commented_violations() {
        let contents = "// let _ = session.history.unwrap();";
        let mut violations = Vec::new();
        lint_file_contents(std::path::Path::new("test.rs"), contents, &mut violations);
        assert!(violations.is_empty());
    }
}
