//! Stub remote memory providers (Honcho, Hindsight, Mem0).
//!
//! These providers implement [`MemoryProvider`] so users can select them in
//! settings, but they require external API configuration to be functional.
//! Each provider returns empty results and logs a warning when called without
//! configuration.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use super::{ExtensionMetadata, PanelSlot};
use super::memory_provider::{MemoryEntry, MemoryProvider, MemoryQuery};

/// Honcho memory provider stub.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct HonchoMemoryProvider;

/// Hindsight memory provider stub.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct HindsightMemoryProvider;

/// Mem0 memory provider stub.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct Mem0MemoryProvider;

impl MemoryProvider for HonchoMemoryProvider {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "memory-honcho".into(),
            name: "Honcho memory".into(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Right),
            enabled: false,
        }
    }

    fn list(&self, _query: &MemoryQuery) -> Result<Vec<MemoryEntry>, String> {
        Ok(Vec::new())
    }

    fn set(&mut self, _key: String, _value: String, _category: String) -> Result<(), String> {
        Err("Honcho provider is not configured. Set the API endpoint and key in settings.".into())
    }

    fn delete(&mut self, _key: &str) -> Result<bool, String> {
        Err("Honcho provider is not configured. Set the API endpoint and key in settings.".into())
    }

    fn recall(&self, _conversation_summary: &str, _limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        Ok(Vec::new())
    }
}

impl MemoryProvider for HindsightMemoryProvider {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "memory-hindsight".into(),
            name: "Hindsight memory".into(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Right),
            enabled: false,
        }
    }

    fn list(&self, _query: &MemoryQuery) -> Result<Vec<MemoryEntry>, String> {
        Ok(Vec::new())
    }

    fn set(&mut self, _key: String, _value: String, _category: String) -> Result<(), String> {
        Err(
            "Hindsight provider is not configured. Set the API endpoint and key in settings."
                .into(),
        )
    }

    fn delete(&mut self, _key: &str) -> Result<bool, String> {
        Err(
            "Hindsight provider is not configured. Set the API endpoint and key in settings."
                .into(),
        )
    }

    fn recall(&self, _conversation_summary: &str, _limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        Ok(Vec::new())
    }
}

impl MemoryProvider for Mem0MemoryProvider {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "memory-mem0".into(),
            name: "Mem0 memory".into(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Right),
            enabled: false,
        }
    }

    fn list(&self, _query: &MemoryQuery) -> Result<Vec<MemoryEntry>, String> {
        Ok(Vec::new())
    }

    fn set(&mut self, _key: String, _value: String, _category: String) -> Result<(), String> {
        Err("Mem0 provider is not configured. Set the API key in settings.".into())
    }

    fn delete(&mut self, _key: &str) -> Result<bool, String> {
        Err("Mem0 provider is not configured. Set the API key in settings.".into())
    }

    fn recall(&self, _conversation_summary: &str, _limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        Ok(Vec::new())
    }
}
