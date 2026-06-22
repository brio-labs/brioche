//! Commands for extensibility features including memory, skills, tools, settings sections, and metrics.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_shell_persistence::extensions::ExtensionMetadata;
use brioche_shell_persistence::extensions::context::CompressorContextEngine;
use brioche_shell_persistence::extensions::footer::{FooterContext, FooterMetric};
use brioche_shell_persistence::extensions::memory_provider::{
    LocalMemoryProvider, MemoryEntry, MemoryProvider, MemoryQuery,
};
use brioche_shell_persistence::extensions::settings_sections::SettingsSection;
use brioche_shell_persistence::extensions::tool_provider::{ToolDescriptor, UserToolDefinition};
use brioche_shell_persistence::{Settings, skills};
use serde::Serialize;
use tauri::State;

use crate::state::DesktopState;

// ---------------------------------------------------------------------------
// Memory commands
// ---------------------------------------------------------------------------

/// Memory entry payload for the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated string representations of memory entries. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
pub struct MemoryEntryPayload {
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

impl From<&MemoryEntry> for MemoryEntryPayload {
    fn from(entry: &MemoryEntry) -> Self {
        Self {
            key: entry.key.clone(),
            value: entry.value.clone(),
            category: entry.category.clone(),
            created_at: entry.created_at,
            updated_at: entry.updated_at,
            access_count: entry.access_count,
        }
    }
}

/// Lists all memory entries, optionally filtered by category.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the total number of memories stored locally. Reads disk.
///
/// # Panic / Safety
/// Never panics. Returns Err on load or listing failure.
#[tauri::command]
pub async fn list_memories(category: Option<String>) -> Result<Vec<MemoryEntryPayload>, String> {
    let store = LocalMemoryProvider::load();
    let query = MemoryQuery {
        category,
        query: None,
    };
    let entries = store.list(&query)?;
    Ok(entries.iter().map(MemoryEntryPayload::from).collect())
}

/// Sets (adds or updates) a memory entry.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the number of local memories. Reads and writes disk.
///
/// # Panic / Safety
/// Never panics. Returns Err on save failure.
#[tauri::command]
pub async fn set_memory(key: String, value: String, category: String) -> Result<(), String> {
    let mut store = LocalMemoryProvider::load();
    store.set(key, value, category)
}

/// Deletes a memory entry by key.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the number of local memories. Reads and writes disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if memory key is not found or save fails.
#[tauri::command]
pub async fn delete_memory(key: String) -> Result<(), String> {
    let mut store = LocalMemoryProvider::load();
    if !store.delete(&key)? {
        return Err(format!("Memory '{}' not found", key));
    }
    Ok(())
}

/// Searches memory entries by key or value.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is the total number of memories stored locally. Reads disk.
///
/// # Panic / Safety
/// Never panics. Returns Err on load or listing failure.
#[tauri::command]
pub async fn search_memories(query: String) -> Result<Vec<MemoryEntryPayload>, String> {
    let store = LocalMemoryProvider::load();
    let query = MemoryQuery {
        category: None,
        query: Some(query),
    };
    let results = store.list(&query)?;
    Ok(results.iter().map(MemoryEntryPayload::from).collect())
}

// ---------------------------------------------------------------------------
// Skills commands
// ---------------------------------------------------------------------------

/// Skill payload for the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated skill configurations. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
pub struct SkillPayload {
    /// Machine-readable skill identifier (e.g., "system-prompt").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Semantic version of the skill.
    pub version: String,
    /// Author or maintainer of the skill.
    pub author: String,
    /// SPDX license identifier.
    pub license: String,
    /// Supported platforms (e.g., ["linux", "macos", "windows"]).
    pub platforms: Vec<String>,
    /// Category for grouping (e.g., "system", "devops").
    pub category: String,
    /// Absolute filesystem path to the skill directory.
    pub path: String,
    /// Tags for filtering and search.
    pub tags: Vec<String>,
    /// Names of related skills.
    pub related_skills: Vec<String>,
    /// Full markdown content of the skill.
    pub content: String,
}

impl From<&skills::Skill> for SkillPayload {
    fn from(s: &skills::Skill) -> Self {
        Self {
            name: s.name.clone(),
            description: s.description.clone(),
            version: s.version.clone(),
            author: s.author.clone(),
            license: s.license.clone(),
            platforms: s.platforms.clone(),
            category: s.category.clone(),
            path: s.path.clone(),
            tags: s.tags.clone(),
            related_skills: s.related_skills.clone(),
            content: s.content.clone(),
        }
    }
}

/// Lists all discovered skills.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(K) where K is the number of skills. Performs file scanning.
///
/// # Panic / Safety
/// Never panics. Returns empty list if scanning fails.
#[tauri::command]
pub async fn list_skills() -> Result<Vec<SkillPayload>, String> {
    let skills = skills::scan_skills();
    Ok(skills.iter().map(SkillPayload::from).collect())
}

/// Gets the content of a specific skill.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S) where S is the size of the SKILL.md file. Reads disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if skill content cannot be read.
#[tauri::command]
pub async fn get_skill_content(name: String) -> Result<String, String> {
    skills::read_skill_content(&name).ok_or_else(|| format!("Skill '{}' not found", name))
}

/// Reads a linked file from a skill directory.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(F) where F is the target file size. Reads disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if the file is missing or reading fails.
#[tauri::command]
pub async fn get_skill_file(name: String, file_path: String) -> Result<String, String> {
    skills::read_skill_file(&name, &file_path)
        .ok_or_else(|| format!("File '{}' not found in skill '{}'", file_path, name))
}

/// Enables or disables a skill.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S) where S is the number of skill providers. Writes configuration.
///
/// # Panic / Safety
/// Never panics. Returns Err if target skill provider is not found or not mutable.
#[tauri::command]
pub async fn set_skill_enabled(
    state: State<'_, DesktopState>,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    let mut registry = state.extensions.write().await;
    for provider in registry.skill_providers_mut() {
        if let Some(provider) = Arc::get_mut(provider) {
            return provider.set_enabled(&name, enabled);
        }
    }
    Err("No mutable skill provider available".into())
}

/// Creates a new skill package.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(C) where C is the size of the skill contents to write to disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if skill directory cannot be created or file write fails.
#[tauri::command]
pub async fn create_skill(
    state: State<'_, DesktopState>,
    name: String,
    category: String,
    description: String,
    content: String,
) -> Result<(), String> {
    let mut registry = state.extensions.write().await;
    for provider in registry.skill_providers_mut() {
        if let Some(provider) = Arc::get_mut(provider) {
            return provider.create_skill(&name, &category, &description, &content);
        }
    }
    Err("No mutable skill provider available".into())
}

/// Deletes a skill package.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) file system deletion.
///
/// # Panic / Safety
/// Never panics. Returns Err if the skill package does not exist or deletion fails.
#[tauri::command]
pub async fn delete_skill(state: State<'_, DesktopState>, name: String) -> Result<(), String> {
    let mut registry = state.extensions.write().await;
    for provider in registry.skill_providers_mut() {
        if let Some(provider) = Arc::get_mut(provider) {
            return provider.delete_skill(&name);
        }
    }
    Err("No mutable skill provider available".into())
}

// ---------------------------------------------------------------------------
// Extension registry & settings section commands
// ---------------------------------------------------------------------------

/// Returns metadata for all registered desktop extensions.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(E) where E is the number of loaded extensions. Reads extension state.
///
/// # Panic / Safety
/// Never panics. Returns Err on lock failures.
#[tauri::command]
pub async fn list_extensions(
    state: State<'_, DesktopState>,
) -> Result<Vec<ExtensionMetadata>, String> {
    let registry = state.extensions.read().await;
    Ok(registry.metadata().to_vec())
}

/// Returns all settings sections contributed by extensions.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S log S) where S is the number of settings sections. Reads extension registry and sorts them.
///
/// # Panic / Safety
/// Never panics. Returns Err on lock failures.
#[tauri::command]
pub async fn list_settings_sections(
    state: State<'_, DesktopState>,
) -> Result<Vec<SettingsSection>, String> {
    let registry = state.extensions.read().await;
    let mut sections: Vec<_> = registry
        .settings_sections()
        .iter()
        .flat_map(|p| p.sections())
        .collect();
    sections.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
    Ok(sections)
}

/// Computes footer metrics from all registered providers.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(M) where M is the number of footer metrics. Reads configurations, locks session manager, estimates history tokens.
///
/// # Panic / Safety
/// Never panics. Returns Err if lock or computing fails.
#[tauri::command]
pub async fn get_footer_metrics(
    state: State<'_, DesktopState>,
) -> Result<Vec<FooterMetric>, String> {
    let settings = Settings::load();
    let registry = state.extensions.read().await;
    let mgr = state.manager.read().await;
    let current_model = settings.chat_model();

    let estimated_tokens: usize = if let Some(manager) = mgr.as_ref() {
        if let Some(entry) = manager.get(manager.current_id()) {
            let history = entry.history.read().await;
            CompressorContextEngine::estimate_tokens(&history)
        } else {
            0
        }
    } else {
        0
    };
    let context_remaining = settings.context_window().saturating_sub(estimated_tokens) as i64;

    let context_note = match state.last_context_note.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => None,
    };

    let ctx = FooterContext {
        version: env!("CARGO_PKG_VERSION").to_string(),
        session_started_at: crate::commands::shell::session_started_at(),
        current_model,
        context_remaining,
        context_note,
    };

    let mut metrics: Vec<_> = registry
        .footer_metrics()
        .iter()
        .map(|m| m.compute(&ctx))
        .collect();
    metrics.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    Ok(metrics)
}

// ---------------------------------------------------------------------------
// Tool commands
// ---------------------------------------------------------------------------

/// Returns all tools from all registered providers.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(T log T) where T is the total number of tools. Reads provider tools and sorts them.
///
/// # Panic / Safety
/// Never panics. Returns Err on registry lock failures.
#[tauri::command]
pub async fn list_tools(state: State<'_, DesktopState>) -> Result<Vec<ToolDescriptor>, String> {
    let registry = state.extensions.read().await;
    let mut tools = Vec::new();
    for provider in registry.tool_providers() {
        tools.extend(provider.tools());
    }
    tools.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tools)
}

/// Enables or disables a tool.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of tool providers. Writes tool settings to registry.
///
/// # Panic / Safety
/// Never panics. Returns Err if target tool provider is not found or not mutable.
#[tauri::command]
pub async fn set_tool_enabled(
    state: State<'_, DesktopState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut registry = state.extensions.write().await;
    for provider in registry.tool_providers_mut() {
        if let Some(provider) = Arc::get_mut(provider) {
            provider.set_enabled(&id, enabled)?;
            return Ok(());
        }
    }
    Err(format!("Tool provider not available for '{}'", id))
}

/// Adds a user-defined tool.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of tool providers. Appends custom tool and writes configurations to disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if no mutable tool provider is found or tool name overlaps.
#[tauri::command]
pub async fn add_user_tool(
    state: State<'_, DesktopState>,
    tool: UserToolDefinition,
) -> Result<(), String> {
    let mut registry = state.extensions.write().await;
    for provider in registry.tool_providers_mut() {
        if let Some(provider) = Arc::get_mut(provider) {
            return provider.add_user_tool(tool);
        }
    }
    Err("No mutable tool provider available".into())
}

/// Removes a user-defined tool.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of tool providers. Searches and removes custom tool, then writes changes to disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if target tool not found or no mutable provider available.
#[tauri::command]
pub async fn remove_user_tool(state: State<'_, DesktopState>, id: String) -> Result<(), String> {
    let mut registry = state.extensions.write().await;
    for provider in registry.tool_providers_mut() {
        if let Some(provider) = Arc::get_mut(provider) {
            return provider.remove_user_tool(&id);
        }
    }
    Err("No mutable tool provider available".into())
}
