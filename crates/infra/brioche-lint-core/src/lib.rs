//! Shared infrastructure for Brioche lint tooling — Book V.
//!
//! Provides directory walking, source-file filtering, lightweight
//! reporting helpers, and a reusable path CLI argument for
//! `cargo-brioche-lint` and `cargo-brioche-lint-invariants`.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, §3.5

use std::path::PathBuf;

pub mod report;
pub mod walk;

/// Exit codes returned by lint binaries.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
#[derive(Debug, Clone, Copy)]
pub enum ExitCode {
    /// No violations found.
    Success = 0,
    /// One or more violations found.
    Violations = 1,
    /// Internal error prevented completion.
    InternalError = 2,
}

/// Reusable CLI path argument shared by lint binaries.
///
/// `--path` is the primary flag; `--root` is a visible alias for
/// backwards compatibility with `cargo-brioche-lint-invariants`.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
#[derive(clap::Args, Debug)]
pub struct PathArg {
    /// Path to scan (default: current directory).
    #[arg(
        long,
        short,
        default_value = ".",
        visible_alias = "root",
        global = true
    )]
    pub path: PathBuf,
}
