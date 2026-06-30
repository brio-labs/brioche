//! Directory walking utilities for Brioche linting tools.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, docs/SPECS.md §Book IV Ch 3 §3.5

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Walk `root` and return paths to source files matching `extensions`.
///
/// Skips directories named `target` or `.git` to avoid build artifacts
/// and version-control metadata.
///
/// # Complexity
/// O(n) where n is the number of filesystem entries visited.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn source_files<'a>(
    root: impl AsRef<Path>,
    extensions: &'a [&'a str],
) -> impl Iterator<Item = PathBuf> + 'a {
    let root = root.as_ref().to_path_buf();
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(move |entry| {
            let path = entry.path();
            let ext_ok = path
                .extension()
                .is_some_and(|ext| extensions.iter().any(|e| ext == *e));
            ext_ok
                && !path.components().any(|component| {
                    let name = component.as_os_str().to_string_lossy();
                    name == "target" || name == ".git"
                })
        })
        .map(|entry| entry.path().to_path_buf())
}
