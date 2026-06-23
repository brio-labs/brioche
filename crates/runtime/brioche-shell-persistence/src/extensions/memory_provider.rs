//! Modular memory provider extension point.
//!
//! Generic AMP Core-compatible memory endpoints and custom providers can be
//! added by implementing [`MemoryProvider`]. The desktop ships with
//! [`LocalMemoryProvider`] as the default, backed by a JSON file in the user's
//! config directory.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::{Deserialize, Serialize};

use super::{ExtensionMetadata, PanelSlot};

/// A memory entry returned by a provider.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Stack-allocated struct with heap-allocated String fields. O(1) creation.
///
/// # Panic / Safety
/// Never panics.
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
    /// Provider that owns this entry.
    pub provider_id: String,
}

/// Query sent to a memory provider.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Stack-allocated query filters. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Default)]
pub struct MemoryQuery {
    /// Optional category filter.
    pub category: Option<String>,
    /// Optional free-text search query.
    pub query: Option<String>,
}

/// Lifecycle context passed to a memory provider at session start.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Stack-allocated parameters. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Default)]
pub struct MemorySessionContext {
    /// Stable session identifier.
    pub session_id: String,
    /// Workspace / project identifier.
    pub workspace: String,
    /// User identifier for multi-tenant scoping.
    pub user_id: Option<String>,
    /// Agent identity / persona.
    pub agent_id: Option<String>,
}

/// A memory-provider extension trait.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Implementation dependent.
///
/// # Panic / Safety
/// Implementation dependent.
pub trait MemoryProvider: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;

    /// Called once per session so the provider can scope its operations.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn initialize(&self, _ctx: MemorySessionContext) -> Result<(), String> {
        Ok(())
    }

    /// Lists entries matching the query.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn list(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, String>;

    /// Sets (adds or updates) an entry.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn set(&mut self, key: String, value: String, category: String) -> Result<(), String>;

    /// Deletes an entry by key.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn delete(&mut self, key: &str) -> Result<bool, String>;

    /// Returns entries that may be relevant for the current conversation.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn recall(&self, conversation_summary: &str, limit: usize) -> Result<Vec<MemoryEntry>, String>;

    /// Optional tool schemas this provider wants to expose to the model.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn tool_schemas(&self) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// Handle a tool call emitted by the model for one of this provider's tools.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn handle_tool_call(
        &self,
        _tool_name: &str,
        _args: serde_json::Value,
    ) -> Result<String, String> {
        Err("Tool calls not supported by this provider".into())
    }

    /// Called when the agent's built-in memory tool fires, so external backends can mirror the write.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn on_memory_write(
        &mut self,
        _key: String,
        _value: String,
        _category: String,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Called before context compression discards old messages.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn on_pre_compress(&mut self, _messages: &[MemoryEntry]) -> Result<(), String> {
        Ok(())
    }

    /// Called at session end so the provider can flush or summarize.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn on_session_end(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// Default local memory provider storing memories as JSON on disk.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// In-memory storage uses a vector of entries. Operations are linear with number of entries.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalMemoryProvider {
    entries: Vec<MemoryEntry>,
}

impl LocalMemoryProvider {
    /// Loads the local memory store from disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the size of the JSON file on disk. Performs blocking file I/O.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if the file cannot be read or parsed.
    pub fn load() -> Result<Self, String> {
        let path = memory_path();
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read memory store: {e}"))?;
        serde_json::from_str::<LocalMemoryProvider>(&data)
            .map_err(|e| format!("Failed to parse memory store: {e}"))
    }

    /// Saves the local memory store to disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the size of the serialized memory entries. Performs blocking file I/O.
    ///
    /// # Panic / Safety
    /// Never panics. Returns error String on write failure.
    pub fn save(&self) -> Result<(), String> {
        let path = memory_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create memory dir: {e}"))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize memory: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write memory: {e}"))
    }
}

impl MemoryProvider for LocalMemoryProvider {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "memory-local".into(),
            name: "Local memory".into(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Right),
            enabled: true,
        }
    }

    fn list(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, String> {
        let q = match query.query.as_ref() {
            Some(s) => s.to_lowercase(),
            None => String::new(),
        };
        Ok(self
            .entries
            .iter()
            .filter(|e| {
                let matches_category = query
                    .category
                    .as_ref()
                    .is_none_or(|c| e.category.eq_ignore_ascii_case(c));
                let matches_query = q.is_empty()
                    || e.key.to_lowercase().contains(&q)
                    || e.value.to_lowercase().contains(&q);
                matches_category && matches_query
            })
            .cloned()
            .collect())
    }

    fn set(&mut self, key: String, value: String, category: String) -> Result<(), String> {
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
                provider_id: "memory-local".into(),
            });
        }
        self.save()
    }

    fn delete(&mut self, key: &str) -> Result<bool, String> {
        let len = self.entries.len();
        self.entries.retain(|e| e.key != key);
        let removed = self.entries.len() < len;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    fn recall(&self, conversation_summary: &str, limit: usize) -> Result<Vec<MemoryEntry>, String> {
        let q = conversation_summary.to_lowercase();
        let mut scored: Vec<(usize, &MemoryEntry)> = self
            .entries
            .iter()
            .map(|e| {
                let score = if e.value.to_lowercase().contains(&q) {
                    2
                } else if e.key.to_lowercase().contains(&q) {
                    1
                } else {
                    0
                };
                (score, e)
            })
            .filter(|(s, _)| *s > 0)
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.access_count.cmp(&a.1.access_count))
        });
        Ok(scored
            .into_iter()
            .take(limit)
            .map(|(_, e)| e.clone())
            .collect())
    }
}

fn system_time_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
}

fn memory_path() -> std::path::PathBuf {
    let config_dir = match dirs::config_dir() {
        Some(d) => d,
        None => std::env::temp_dir(),
    };
    config_dir.join("brioche-desktop").join("memory.json")
}
