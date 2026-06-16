//! Persistent memory for the desktop app.
//!
//! Memory entries are stored as JSON in the user's config directory:
//! - Linux:   ~/.config/brioche-desktop/memory.json
//! - macOS:   ~/Library/Application Support/brioche-desktop/memory.json
//! - Windows: %APPDATA%\brioche-desktop\memory.json
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

/// A single memory entry.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique key for the memory entry.
    pub key: String,
    /// Value/content of the memory entry.
    pub value: String,
    /// Category for grouping (e.g., "user", "project").
    pub category: String,
    /// Unix timestamp when the entry was created.
    pub created_at: u64,
    /// Unix timestamp when the entry was last updated.
    pub updated_at: u64,
    /// Number of times this entry has been accessed.
    pub access_count: u32,
}

/// In-memory store for user-defined key-value entries.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MemoryStore {
    entries: Vec<MemoryEntry>,
}

impl MemoryStore {
    /// Loads the memory store from disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn load() -> Self {
        let path = memory_path();
        if let Ok(data) = std::fs::read_to_string(&path)
            && let Ok(store) = serde_json::from_str::<MemoryStore>(&data)
        {
            return store;
        }
        Self::default()
    }

    /// Saves the memory store to disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn save(&self) -> Result<(), String> {
        let path = memory_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create memory dir: {e}"))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize memory: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write memory: {e}"))?;
        Ok(())
    }

    /// Adds or updates a memory entry.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn set(&mut self, key: String, value: String, category: String) {
        let now = system_time_secs();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.key == key) {
            entry.value = value;
            entry.category = category;
            entry.updated_at = now;
        } else {
            self.entries.push(MemoryEntry {
                key,
                value,
                category,
                created_at: now,
                updated_at: now,
                access_count: 0,
            });
        }
    }

    /// Gets a memory entry by key, incrementing its access count.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn get(&mut self, key: &str) -> Option<&MemoryEntry> {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.key == key) {
            entry.access_count += 1;
            return Some(entry as &MemoryEntry);
        }
        None
    }

    /// Gets a memory value by key.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn get_value(&mut self, key: &str) -> Option<String> {
        self.get(key).map(|e| e.value.clone())
    }

    /// Deletes a memory entry by key.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn delete(&mut self, key: &str) -> bool {
        let len = self.entries.len();
        self.entries.retain(|e| e.key != key);
        self.entries.len() < len
    }

    /// Lists all memory entries, optionally filtered by category.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn list(&self, category: Option<&str>) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| match category {
                Some(c) => e.category == c,
                None => true,
            })
            .collect()
    }

    /// Searches memory entries by key or value.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.key.to_lowercase().contains(&q) || e.value.to_lowercase().contains(&q))
            .collect()
    }
}

fn system_time_secs() -> u64 {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}

fn memory_path() -> PathBuf {
    let config_dir = match dirs::config_dir() {
        Some(d) => d,
        None => std::env::temp_dir(),
    };
    config_dir.join("brioche-desktop").join("memory.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_set_and_get() {
        let mut store = MemoryStore::default();
        store.set("key1".into(), "value1".into(), "test".into());
        assert_eq!(store.get_value("key1"), Some("value1".into()));
    }

    #[test]
    fn memory_store_updates_existing() {
        let mut store = MemoryStore::default();
        store.set("key1".into(), "value1".into(), "test".into());
        store.set("key1".into(), "value2".into(), "test".into());
        assert_eq!(store.get_value("key1"), Some("value2".into()));
        assert_eq!(store.entries.len(), 1);
    }

    #[test]
    fn memory_store_delete() {
        let mut store = MemoryStore::default();
        store.set("key1".into(), "value1".into(), "test".into());
        assert!(store.delete("key1"));
        assert!(!store.delete("key1"));
    }

    #[test]
    fn memory_store_search() {
        let mut store = MemoryStore::default();
        store.set("hello".into(), "world".into(), "test".into());
        store.set("foo".into(), "bar".into(), "test".into());
        let results = store.search("hello");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "hello");
    }
}
