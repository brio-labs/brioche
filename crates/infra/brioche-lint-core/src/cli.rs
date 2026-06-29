//! Shared CLI argument helpers for Brioche linting tools.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, docs/SPECS.md §Book IV Ch 3 §3.5

use clap::Parser;
use std::path::PathBuf;

/// Root directory argument shared by linting tools.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
#[derive(Parser, Debug)]
pub struct RootArgs {
    /// Root directory to scan.
    #[arg(
        long,
        short,
        visible_alias = "path",
        global = true,
        default_value = "."
    )]
    pub root: PathBuf,
}

/// Output format argument shared by linting tools.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
#[derive(Parser, Debug)]
pub struct FormatArgs {
    /// Output format.
    #[arg(long, global = true, default_value = "text")]
    pub format: String,
}
