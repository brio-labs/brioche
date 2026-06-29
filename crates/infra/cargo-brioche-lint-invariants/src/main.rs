//! `cargo-brioche-lint-invariants` — invariant reference checker — Book V.
//!
//! Scans Rust source files for `Refs:` documentation patterns and
//! validates them against the canonical invariant registry.
//!
//! ## Usage
//! ```text
//! cargo brioche-lint-invariants check-refs
//! cargo brioche-lint-invariants check-matrix --json
//! ```
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4

use std::fs;

use brioche_lint_core::{
    cli::{FormatArgs, RootArgs},
    report::print_json,
    walk::source_files,
};
use clap::{Parser, Subcommand};
use regex::Regex;

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
/// - `SPECS` refers to docs/SPECS.md (the canonical architecture specification).
/// - `docs/SPECS` refers to docs/SPECS.md (the canonical architecture specification).
/// - `SCIFI` refers to the SCIFI compositional design methodology
///   (PHILOSOPHY.md §7.2).
const KNOWN_CANONICAL_REFS: &[&str] = &["SPECS", "docs/SPECS", "SCIFI"];

/// Returns true if `inv` starts with any of the known category prefixes.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
fn has_known_category(inv: &str) -> bool {
    KNOWN_CATEGORIES.iter().any(|cat| inv.starts_with(*cat))
}

/// Returns true if `inv` is a known canonical non-invariant reference.
///
/// Normalizes `docs/SPECS.md` → `docs/SPECS` before checking.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
fn is_known_canonical_ref(inv: &str) -> bool {
    let normalized = match inv.strip_suffix(".md") {
        Some(s) => s,
        None => inv,
    };
    KNOWN_CANONICAL_REFS.contains(&normalized)
}

/// CLI arguments.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
#[derive(Parser)]
#[command(name = "cargo-brioche-lint-invariants")]
#[command(about = "Check Brioche invariant references in source code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Root directory to scan.
    #[command(flatten)]
    root: RootArgs,

    /// Output format.
    #[command(flatten)]
    format: FormatArgs,
}

/// Subcommands.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
#[derive(Subcommand)]
enum Commands {
    /// Check that all `Refs:` entries match known invariant patterns.
    CheckRefs,
    /// Validate the governance compatibility matrix.
    CheckMatrix,
}

/// A single invariant reference found in source.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
#[derive(Debug, serde::Serialize)]
struct RefEntry {
    file: String,
    line: usize,
    invariant: String,
    valid: bool,
}

/// Lint result for a single file.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
#[derive(Debug, serde::Serialize)]
struct FileResult {
    file: String,
    refs: Vec<RefEntry>,
    unknown_refs: Vec<String>,
}

/// Entry point.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::CheckRefs => {
            let results = match check_refs(&cli.root.root) {
                Ok(r) => r,
                Err(err) => {
                    eprintln!("internal error: {err}");
                    std::process::exit(2);
                }
            };
            if cli.format.format == "json" {
                print_json(&results);
            } else {
                print_text(&results);
            }
            let has_errors = results.iter().any(|r| !r.unknown_refs.is_empty());
            std::process::exit(if has_errors { 1 } else { 0 });
        }
        Commands::CheckMatrix => {
            let violations = check_matrix();
            if violations.is_empty() {
                println!("Governance compatibility matrix check: OK");
                std::process::exit(0);
            }
            println!(
                "Governance compatibility matrix check: {} issue(s)",
                violations.len()
            );
            for v in &violations {
                println!("  - {v}");
            }
            std::process::exit(1);
        }
    }
}

/// Scan source files for `Refs:` patterns.
///
/// # Complexity
/// O(n · m) where n = files scanned, m = lines per file.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
fn check_refs(root: &std::path::Path) -> Result<Vec<FileResult>, String> {
    let ref_re = Regex::new(r"Refs:\s*([A-Za-z0-9_/.-]+(?:\s*,\s*[A-Za-z0-9_/.-]+)*)")
        .map_err(|e| format!("failed to compile ref regex: {e}"))?;
    let invariant_re = Regex::new(r"^I-[A-Za-z]+-[A-Za-z0-9_-]+$")
        .map_err(|e| format!("failed to compile invariant regex: {e}"))?;

    let mut results = Vec::new();

    for path in source_files(root, &["rs", "md"]) {
        let contents = match fs::read_to_string(&path) {
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

    Ok(results)
}

/// Validate the governance compatibility matrix.
///
/// Checks that no entries are duplicated and that every
/// `Incompatible` entry carries an explanatory note.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
fn check_matrix() -> Vec<String> {
    use brioche_governance_default::{CompatibilityLevel, GovernanceCompatibilityMatrix};

    let entries = GovernanceCompatibilityMatrix::entries();
    let mut violations = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for entry in entries {
        if entry.trait_a.is_empty()
            || entry.impl_a.is_empty()
            || entry.trait_b.is_empty()
            || entry.impl_b.is_empty()
        {
            violations.push(format!(
                "empty trait/impl name in compatibility entry: {:?}",
                entry
            ));
        }

        if entry.level == CompatibilityLevel::Incompatible && entry.note.is_none() {
            violations.push(format!(
                "Incompatible entry without note: {}::{} x {}::{}",
                entry.trait_a, entry.impl_a, entry.trait_b, entry.impl_b
            ));
        }

        let key = (entry.trait_a, entry.impl_a, entry.trait_b, entry.impl_b);
        if !seen.insert(key) {
            violations.push(format!(
                "duplicate compatibility entry: {}::{} x {}::{}",
                entry.trait_a, entry.impl_a, entry.trait_b, entry.impl_b
            ));
        }
    }

    violations
}

/// Print results as human-readable text.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_invariant_refs() {
        let _contents = "/// Refs: I-Core-Pure, I-Gov-Decision-Required";
        let _root = std::path::Path::new(".");
        // Unit-test the regex directly by calling check_refs on a temp dir would
        // be noisy; instead exercise validation helpers.
        assert!(has_known_category("I-Core-Pure"));
        assert!(has_known_category("I-Gov-Decision-Required"));
        assert!(is_known_canonical_ref("SPECS"));
        assert!(!is_known_canonical_ref("UNKNOWN"));
    }

    #[test]
    fn rejects_unknown_categories() {
        assert!(!has_known_category("I-Unknown-Thing"));
    }

    #[test]
    fn check_matrix_has_no_violations() {
        let violations = check_matrix();
        assert!(
            violations.is_empty(),
            "expected no matrix violations, got: {violations:?}"
        );
    }
}
