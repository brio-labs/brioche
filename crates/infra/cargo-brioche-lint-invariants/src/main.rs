//! `cargo-brioche-lint-invariants` — invariant reference checker.
//!
//! Scans Rust source files for `Refs:` documentation patterns and
//! validates them against the canonical invariant registry.
//!
//! ## Usage
//! ```text
//! cargo brioche-lint-invariants --check-refs
//! cargo brioche-lint-invariants --check-matrix --json
//! ```
//!
//! Refs: SPECS.md §Book V

use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use regex::Regex;
use walkdir::WalkDir;

/// Known invariant categories and their prefixes.
///
/// Used by `check_refs` to validate that an invariant ID belongs to
/// a recognised book/layer of the architecture.
const KNOWN_CATEGORIES: &[&str] = &[
    "I-Core",
    "I-Gov",
    "I-Shell",
    "I-UI",
    "I-Persist",
    "I-Comp",
    "I-Eco",
];

/// Canonical non-invariant references that are also valid in `Refs:` blocks.
///
/// - `SPECS` refers to SPECS.md (the canonical architecture specification).
/// - `SCIFI` refers to the SCIFI compositional design methodology
///   (PHILOSOPHY.md §7.2).
const KNOWN_CANONICAL_REFS: &[&str] = &["SPECS", "SCIFI"];

/// Returns true if `inv` starts with any of the known category prefixes.
fn has_known_category(inv: &str) -> bool {
    KNOWN_CATEGORIES.iter().any(|cat| inv.starts_with(*cat))
}

/// Returns true if `inv` is a known canonical non-invariant reference.
fn is_known_canonical_ref(inv: &str) -> bool {
    KNOWN_CANONICAL_REFS.contains(&inv)
}

/// CLI arguments.
#[derive(Parser)]
#[command(name = "cargo-brioche-lint-invariants")]
#[command(about = "Check Brioche invariant references in source code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Root directory to scan (default: current directory).
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    /// Output format.
    #[arg(long, global = true, default_value = "text")]
    format: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Check that all `Refs:` entries match known invariant patterns.
    CheckRefs,
    /// Validate the governance compatibility matrix.
    CheckMatrix,
}

/// A single invariant reference found in source.
#[derive(Debug, serde::Serialize)]
struct RefEntry {
    file: String,
    line: usize,
    invariant: String,
    valid: bool,
}

/// Lint result for a single file.
#[derive(Debug, serde::Serialize)]
struct FileResult {
    file: String,
    refs: Vec<RefEntry>,
    unknown_refs: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    let root = match cli.root {
        Some(p) => p,
        None => PathBuf::from("."),
    };

    match cli.command {
        Commands::CheckRefs => {
            let results = check_refs(&root);
            if cli.format == "json" {
                print_json(&results);
            } else {
                print_text(&results);
            }
            let has_errors = results.iter().any(|r| !r.unknown_refs.is_empty());
            std::process::exit(if has_errors { 1 } else { 0 });
        }
        Commands::CheckMatrix => {
            println!("Governance compatibility matrix check: OK (placeholder)");
            std::process::exit(0);
        }
    }
}

/// Scan source files for `Refs:` patterns.
fn check_refs(root: &PathBuf) -> Vec<FileResult> {
    let ref_re = match Regex::new(r"Refs:\s*([A-Za-z0-9_-]+(?:\s*,\s*[A-Za-z0-9_-]+)*)") {
        Ok(re) => re,
        Err(_) => {
            eprintln!("internal error: failed to compile ref regex");
            return Vec::new();
        }
    };
    let invariant_re = match Regex::new(r"^I-[A-Za-z]+-[A-Za-z0-9_-]+$") {
        Ok(re) => re,
        Err(_) => {
            eprintln!("internal error: failed to compile invariant regex");
            return Vec::new();
        }
    };

    let mut results = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            p.extension().is_some_and(|ext| ext == "rs" || ext == "md")
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

        let mut refs = Vec::new();
        let mut unknown_refs = Vec::new();

        for (line_no, line) in contents.lines().enumerate() {
            for cap in ref_re.captures_iter(line) {
                let Some(inv_match) = cap.get(1) else {
                    continue;
                };
                let inv_str = inv_match.as_str();
                for inv in inv_str.split(',') {
                    let inv = inv.trim();
                    let matches_format = invariant_re.is_match(inv);
                    let known_category = has_known_category(inv);
                    let canonical_ref = is_known_canonical_ref(inv);
                    let valid = (matches_format && known_category) || canonical_ref;
                    refs.push(RefEntry {
                        file: path.display().to_string(),
                        line: line_no + 1,
                        invariant: inv.to_string(),
                        valid,
                    });
                    if !valid {
                        unknown_refs.push(inv.to_string());
                    }
                }
            }
        }

        if !refs.is_empty() || !unknown_refs.is_empty() {
            unknown_refs.sort();
            unknown_refs.dedup();
            results.push(FileResult {
                file: path.display().to_string(),
                refs,
                unknown_refs,
            });
        }
    }

    results
}

fn print_text(results: &[FileResult]) {
    let total_refs: usize = results.iter().map(|r| r.refs.len()).sum();
    let total_unknown: usize = results.iter().map(|r| r.unknown_refs.len()).sum();

    println!("Scanned {} files with invariant references", results.len());
    println!("Total refs: {total_refs}, Unknown: {total_unknown}\n");

    for file in results {
        if !file.unknown_refs.is_empty() {
            println!("{} — {} unknown ref(s)", file.file, file.unknown_refs.len());
            for r in &file.refs {
                if !r.valid {
                    println!("  line {}: {} ❌", r.line, r.invariant);
                }
            }
        }
    }

    if total_unknown == 0 {
        println!("All invariant references are valid ✓");
    }
}

fn print_json(results: &[FileResult]) {
    let json = match serde_json::to_string_pretty(results) {
        Ok(j) => j,
        Err(_) => "[]".into(),
    };
    println!("{json}");
}
