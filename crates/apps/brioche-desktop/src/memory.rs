//! Persistent memory for the desktop app.
//!
//! This module is a thin compatibility wrapper around the extension-system
//! [`LocalMemoryProvider`]. New code should prefer using the provider directly
//! via [`crate::extensions::ExtensionRegistry`].
//!
//! Refs: I-Shell-Runtime-OnlyIO

pub use crate::extensions::memory_provider::{LocalMemoryProvider, MemoryEntry};

/// Loads the local memory store.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn load_store() -> LocalMemoryProvider {
    LocalMemoryProvider::load()
}
