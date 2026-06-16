//! Modular tool provider extension point.
//!
//! Tools no longer have to be embedded in the final binary. The desktop ships a
//! default [`ToolRegistry`] that exposes the standard system tools; users and
//! extensions can register additional tools defined as JSON schema + executor
//! mappings. A tool provider may be local (Rust code), a Wasm module, or an
//! external process.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use super::{ExtensionMetadata, PanelSlot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A tool descriptor.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Machine-readable tool identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Description shown to the model.
    pub description: String,
    /// JSON Schema for the tool parameters.
    pub parameters: serde_json::Value,
    /// Category / folder.
    pub category: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Whether the tool is enabled.
    pub enabled: bool,
    /// Source: `built-in`, `user-json`, `wasm`, `process`.
    pub source: String,
}

/// Extension trait for tool providers.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub trait ToolProvider: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;

    /// Returns all tools provided by this provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn tools(&self) -> Vec<ToolDescriptor>;

    /// Enables or disables a tool by id.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String>;

    /// Adds a user-defined tool.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn add_user_tool(&mut self, tool: UserToolDefinition) -> Result<(), String>;

    /// Removes a user-defined tool.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn remove_user_tool(&mut self, id: &str) -> Result<(), String>;
}

/// A user-defined tool definition.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserToolDefinition {
    /// Tool id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description for the model.
    pub description: String,
    /// JSON Schema object.
    pub parameters: serde_json::Value,
    /// Category.
    pub category: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Executor configuration.
    pub executor: ToolExecutor,
}

/// How a user-defined tool is executed.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutor {
    /// Execute a local shell command. Arguments are interpolated into the
    /// command string with `{arg_name}`.
    Command {
        /// Command template.
        command: String,
        /// Optional working directory.
        working_dir: Option<String>,
    },
    /// POST the arguments as JSON to a URL and return the response body.
    HttpPost {
        /// Target URL.
        url: String,
        /// Additional HTTP headers.
        headers: BTreeMap<String, String>,
    },
    /// Read a file and return its contents (for template-style tools).
    ReadFile {
        /// File path to read.
        path: String,
    },
}

/// Default tool registry.
///
/// Built-in system tools are always available. User-defined tools are persisted
/// in the config directory.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ToolRegistry {
    user_tools: Vec<UserToolDefinition>,
    disabled: Vec<String>,
}

impl ToolRegistry {
    /// Loads the registry from disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn load() -> Self {
        let path = registry_path();
        if let Ok(data) = std::fs::read_to_string(&path)
            && let Ok(registry) = serde_json::from_str::<ToolRegistry>(&data)
        {
            return registry;
        }
        Self::default()
    }

    /// Saves the registry to disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn save(&self) -> Result<(), String> {
        let path = registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create tools dir: {e}"))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize tools: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write tools: {e}"))
    }

    fn built_in_tools() -> Vec<ToolDescriptor> {
        vec![
            ToolDescriptor {
                id: "read_file".into(),
                name: "Read file".into(),
                description: "Read the contents of a file".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path" }
                    },
                    "required": ["path"]
                }),
                category: "filesystem".into(),
                tags: vec!["fs".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "write_file".into(),
                name: "Write file".into(),
                description: "Write content to a file".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" },
                        "append": { "type": "boolean" }
                    },
                    "required": ["path", "content"]
                }),
                category: "filesystem".into(),
                tags: vec!["fs".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "list_dir".into(),
                name: "List directory".into(),
                description: "List files in a directory".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path" }
                    },
                    "required": ["path"]
                }),
                category: "filesystem".into(),
                tags: vec!["fs".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "execute_command".into(),
                name: "Execute command".into(),
                description: "Run a shell command".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "timeout_ms": { "type": "integer" }
                    },
                    "required": ["command"]
                }),
                category: "system".into(),
                tags: vec!["shell".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "fetch_url".into(),
                name: "Fetch URL".into(),
                description: "Fetch content from a URL".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": { "type": "string" }
                    },
                    "required": ["url"]
                }),
                category: "web".into(),
                tags: vec!["http".into()],
                enabled: true,
                source: "built-in".into(),
            },
        ]
    }
}

impl ToolProvider for ToolRegistry {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "tools-default".into(),
            name: "Default tool registry".into(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Left),
            enabled: true,
        }
    }

    fn tools(&self) -> Vec<ToolDescriptor> {
        let mut tools = Self::built_in_tools();
        for user in &self.user_tools {
            tools.push(ToolDescriptor {
                id: user.id.clone(),
                name: user.name.clone(),
                description: user.description.clone(),
                parameters: user.parameters.clone(),
                category: user.category.clone(),
                tags: user.tags.clone(),
                enabled: !self.disabled.contains(&user.id),
                source: "user-json".into(),
            });
        }
        for tool in &mut tools {
            if self.disabled.contains(&tool.id) {
                tool.enabled = false;
            }
        }
        tools
    }

    fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        if enabled {
            self.disabled.retain(|d| d != id);
        } else if !self.disabled.contains(&id.to_string()) {
            self.disabled.push(id.to_string());
        }
        self.save()
    }

    fn add_user_tool(&mut self, tool: UserToolDefinition) -> Result<(), String> {
        if self.user_tools.iter().any(|t| t.id == tool.id) {
            return Err(format!("Tool '{}' already exists", tool.id));
        }
        self.user_tools.push(tool);
        self.save()
    }

    fn remove_user_tool(&mut self, id: &str) -> Result<(), String> {
        let len = self.user_tools.len();
        self.user_tools.retain(|t| t.id != id);
        if self.user_tools.len() == len {
            return Err(format!("Tool '{}' not found", id));
        }
        self.disabled.retain(|d| d != id);
        self.save()
    }
}

fn registry_path() -> std::path::PathBuf {
    let config_dir = match dirs::config_dir() {
        Some(d) => d,
        None => std::env::temp_dir(),
    };
    config_dir.join("brioche-desktop").join("tools.json")
}
