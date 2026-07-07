//! Shared utility helpers for the shell runtime.
//!
//! These small helpers are used by both the runtime itself and by
//! downstream crates (`brioche-shell-persistence`, `brioche-shell-builder`,
//! desktop, terminal) so that trivial routines are not copy-pasted.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Returns seconds since the UNIX epoch.
///
/// Returns `0` if the system clock is before the UNIX epoch, which is
/// treated as a safe sentinel by callers.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) system call.
///
/// # Panic / Safety
/// Never panics.
pub fn system_time_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}

/// Loads a JSON file from disk.
///
/// `context` is used only in error messages so callers can preserve
/// domain-specific diagnostics (e.g. "memory store", "tool registry").
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the file size. Performs blocking file I/O.
///
/// # Panic / Safety
/// Never panics.
pub fn load_json<P, T>(path: P, context: &str) -> Result<T, String>
where
    P: AsRef<Path>,
    T: for<'de> Deserialize<'de>,
{
    let path = path.as_ref();
    let data =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read {context}: {e}"))?;
    serde_json::from_str::<T>(&data).map_err(|e| format!("Failed to parse {context}: {e}"))
}

/// Loads a JSON file from disk, returning `T::default()` on any error.
///
/// This is appropriate for configuration files where a missing or
/// corrupt file should be silently replaced with defaults.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the file size. Performs blocking file I/O.
///
/// # Panic / Safety
/// Never panics.
#[allow(clippy::manual_unwrap_or_default)]
pub fn load_json_or_default<P, T>(path: P) -> T
where
    P: AsRef<Path>,
    T: Default + for<'de> Deserialize<'de>,
{
    match load_json(path, "config") {
        Ok(value) => value,
        Err(_) => T::default(),
    }
}

/// Saves a value to disk as pretty-printed JSON.
///
/// Parent directories are created automatically. `context` is used
/// only in error messages.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the serialized size. Performs blocking file I/O.
///
/// # Panic / Safety
/// Never panics.
pub fn save_json<P, T>(path: P, value: &T, context: &str) -> Result<(), String>
where
    P: AsRef<Path>,
    T: Serialize,
{
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {context} dir: {e}"))?;
    }
    let data = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize {context}: {e}"))?;
    std::fs::write(path, data).map_err(|e| format!("Failed to write {context}: {e}"))
}
