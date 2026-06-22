//! Persistent memory for the desktop app.
//!
//! This module is a thin compatibility wrapper around the extension-system
//! [`LocalMemoryProvider`]. New code should prefer using the provider directly
//! via [`crate::extensions::ExtensionRegistry`].
//!
//! Refs: I-Shell-Runtime-OnlyIO

pub use brioche_shell_persistence::extensions::memory_provider::{
    LocalMemoryProvider, MemoryEntry,
};

/// Loads the local memory store.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the JSON memory database size. Performs blocking disk read.
///
/// # Panic / Safety
/// Never panics. Returns empty memory provider if loading fails.
pub fn load_store() -> LocalMemoryProvider {
    LocalMemoryProvider::load()
}
