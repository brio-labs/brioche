//! Directory walking utilities for Brioche lint tooling.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, §3.5

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Read a source file, returning `None` if it cannot be decoded.
///
/// Lint tools skip unreadable files rather than failing the whole run.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
pub fn read_source_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Returns an iterator over source files under `root`.
///
/// Only includes files whose extension is in `extensions`, and
/// skips directories named `target` or `.git`.
///
/// # Complexity
/// O(n) where n = directory entries under `root`.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
pub fn walk_files<'a>(
    root: &'a Path,
    extensions: &'a [&'a str],
) -> impl Iterator<Item = PathBuf> + 'a {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(move |e| {
            let p = e.path();
            let has_target_extension = p
                .extension()
                .is_some_and(|ext| extensions.iter().any(|target| ext == *target));
            let in_excluded_dir = p.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s == "target" || s == ".git"
            });
            has_target_extension && !in_excluded_dir
        })
        .map(|e| e.path().to_path_buf())
}

/// Walk `root` for Rust source files.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.5
pub fn walk_rust_files(root: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    walk_files(root, &["rs"])
}

/// Walk `root` for Rust and Markdown files.
///
/// Refs: docs/SPECS.md §Book IV Ch 3 §3.4
pub fn walk_rust_and_markdown_files(root: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    walk_files(root, &["rs", "md"])
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn temp_dir_with_files(files: &[(&str, &str)]) -> std::io::Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        for (path, contents) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&full)?;
            file.write_all(contents.as_bytes())?;
        }
        Ok(dir)
    }

    #[test]
    fn walk_rust_files_includes_rs_and_excludes_others() -> std::io::Result<()> {
        let dir = temp_dir_with_files(&[
            ("src/lib.rs", "fn main() {}"),
            ("README.md", "# hello"),
            ("Cargo.toml", "[package]"),
        ])?;
        let paths: Vec<_> = walk_rust_files(dir.path()).collect();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("lib.rs"));
        Ok(())
    }

    #[test]
    fn walk_rust_and_markdown_files_includes_md() -> std::io::Result<()> {
        let dir = temp_dir_with_files(&[("src/lib.rs", "fn main() {}"), ("README.md", "# hello")])?;
        let paths: Vec<_> = walk_rust_and_markdown_files(dir.path()).collect();
        assert_eq!(paths.len(), 2);
        Ok(())
    }

    #[test]
    fn walk_files_skips_target_and_git() -> std::io::Result<()> {
        let dir = temp_dir_with_files(&[
            ("src/lib.rs", "fn main() {}"),
            ("target/debug/foo.rs", "fn foo() {}"),
            (".git/hooks/pre-commit.rs", "fn hook() {}"),
        ])?;
        let paths: Vec<_> = walk_rust_files(dir.path()).collect();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("lib.rs"));
        Ok(())
    }

    #[test]
    fn read_source_file_reads_utf8() -> std::io::Result<()> {
        let dir = temp_dir_with_files(&[("lib.rs", "hello")])?;
        let contents = read_source_file(&dir.path().join("lib.rs"))
            .ok_or_else(|| std::io::Error::other("failed to read source file"))?;
        assert_eq!(contents, "hello");
        Ok(())
    }
}
