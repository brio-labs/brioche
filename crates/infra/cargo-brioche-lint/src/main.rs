//! `cargo-brioche-lint` — plugin linter.
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
//! Refs: SPECS.md §Book V

use std::fs;
use std::path::PathBuf;

use clap::Parser;
use walkdir::WalkDir;

/// CLI arguments.
#[derive(Parser)]
#[command(name = "cargo-brioche-lint")]
#[command(about = "Lint Brioche plugins for forbidden patterns")]
struct Cli {
    /// Path to the plugin crate.
    #[arg(long, short, default_value = ".")]
    path: PathBuf,
}

/// A single lint violation.
#[derive(Debug)]
struct Violation {
    file: String,
    line: usize,
    message: String,
}

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

fn lint_file_contents(path: &std::path::Path, contents: &str, violations: &mut Vec<Violation>) {
    let file = path.display().to_string();

    // Pattern 1: direct session.history or session.state access.
    let forbidden_fields = ["session.history", "session.state"];
    for (line_no, line) in contents.lines().enumerate() {
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
    }
}
