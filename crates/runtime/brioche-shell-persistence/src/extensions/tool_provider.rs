//! Modular tool provider extension point.
//!
//! Tools no longer have to be embedded in the final binary. The desktop ships a
//! default [`ToolRegistry`] that exposes the standard system tools; users and
//! extensions can register additional tools defined as JSON schema + executor
//! mappings. A tool provider may be local (Rust code), a Wasm module, or an
//! external process.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;

use brioche_tools_system::{SystemTool, ToolError};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{ExtensionMetadata, PanelSlot};

/// A tool descriptor.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing metadata. O(1) creation.
///
/// # Panic / Safety
/// Never panics.
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
///
/// # Complexity
/// Implementation dependent.
///
/// # Panic / Safety
/// Implementation dependent.
pub trait ToolProvider: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;

    /// Returns all tools provided by this provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn tools(&self) -> Vec<ToolDescriptor>;

    /// Returns user-defined tool definitions so they can be wired into the shell runtime.
    fn user_tools(&self) -> Vec<UserToolDefinition>;

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
///
/// # Complexity
/// Struct containing metadata and config. O(1) creation.
///
/// # Panic / Safety
/// Never panics.
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
///
/// # Complexity
/// Enum defining execution target. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutor {
    /// Execute a local shell command. Arguments are interpolated into the command string with `{arg_name}`.
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

/// Default tool registry that handles both system tools and custom user tools.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Stores user tools in a Vec. Lookup and search are linear with number of tools.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ToolRegistry {
    user_tools: Vec<UserToolDefinition>,
    disabled: Vec<String>,
}

/// Build a JSON object value from key/value pairs.
fn obj(values: impl IntoIterator<Item = (&'static str, serde_json::Value)>) -> serde_json::Value {
    serde_json::Value::Object(
        values
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<serde_json::Map<String, serde_json::Value>>(),
    )
}

/// Build a JSON array value from items.
fn arr(values: impl IntoIterator<Item = serde_json::Value>) -> serde_json::Value {
    serde_json::Value::Array(values.into_iter().collect())
}

impl ToolRegistry {
    /// Loads the registry from disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the size of the JSON configuration on disk. Performs blocking file I/O.
    ///
    /// # Panic / Safety
    /// Never panics. Returns default empty registry if loading fails.
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
    ///
    /// # Complexity
    /// O(N) where N is the size of the serialized tools. Performs blocking file I/O.
    ///
    /// # Panic / Safety
    /// Never panics. Returns error String on write failure.
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
                parameters: obj([
                    ("type", "object".into()),
                    (
                        "properties",
                        obj([(
                            "path",
                            obj([
                                ("type", "string".into()),
                                ("description", "File path".into()),
                            ]),
                        )]),
                    ),
                    ("required", arr(["path".into()])),
                ]),
                category: "filesystem".into(),
                tags: vec!["fs".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "write_file".into(),
                name: "Write file".into(),
                description: "Write content to a file".into(),
                parameters: obj([
                    ("type", "object".into()),
                    (
                        "properties",
                        obj([
                            ("path", obj([("type", "string".into())])),
                            ("content", obj([("type", "string".into())])),
                            ("append", obj([("type", "boolean".into())])),
                        ]),
                    ),
                    ("required", arr(["path".into(), "content".into()])),
                ]),
                category: "filesystem".into(),
                tags: vec!["fs".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "list_dir".into(),
                name: "List directory".into(),
                description: "List files in a directory".into(),
                parameters: obj([
                    ("type", "object".into()),
                    (
                        "properties",
                        obj([(
                            "path",
                            obj([
                                ("type", "string".into()),
                                ("description", "Directory path".into()),
                            ]),
                        )]),
                    ),
                    ("required", arr(["path".into()])),
                ]),
                category: "filesystem".into(),
                tags: vec!["fs".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "execute_command".into(),
                name: "Execute command".into(),
                description: "Run a shell command".into(),
                parameters: obj([
                    ("type", "object".into()),
                    (
                        "properties",
                        obj([
                            ("command", obj([("type", "string".into())])),
                            ("timeout_ms", obj([("type", "integer".into())])),
                        ]),
                    ),
                    ("required", arr(["command".into()])),
                ]),
                category: "system".into(),
                tags: vec!["shell".into()],
                enabled: true,
                source: "built-in".into(),
            },
            ToolDescriptor {
                id: "fetch_url".into(),
                name: "Fetch URL".into(),
                description: "Fetch content from a URL".into(),
                parameters: obj([
                    ("type", "object".into()),
                    (
                        "properties",
                        obj([("url", obj([("type", "string".into())]))]),
                    ),
                    ("required", arr(["url".into()])),
                ]),
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

    fn user_tools(&self) -> Vec<UserToolDefinition> {
        self.user_tools
            .iter()
            .filter(|t| !self.disabled.contains(&t.id))
            .cloned()
            .collect()
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

/// A [`SystemTool`] implementation that delegates to a user-defined executor.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Wraps UserToolDefinition. O(1) instantiation.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug)]
pub struct UserDefinedTool {
    definition: UserToolDefinition,
}

impl UserDefinedTool {
    /// Creates a wrapper for the given user tool definition.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1).
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn new(definition: UserToolDefinition) -> Self {
        Self { definition }
    }
}

#[async_trait::async_trait]
impl SystemTool for UserDefinedTool {
    fn name(&self) -> String {
        self.definition.id.clone()
    }

    fn description(&self) -> String {
        self.definition.description.clone()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.definition.parameters.clone()
    }

    async fn run(
        &self,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        match &self.definition.executor {
            ToolExecutor::Command {
                command,
                working_dir,
            } => execute_command(command, working_dir.as_deref(), args, cancel).await,
            ToolExecutor::HttpPost { url, headers } => {
                execute_http_post(url, headers, args, cancel).await
            }
            ToolExecutor::ReadFile { path } => execute_read_file(path).await,
        }
    }
}

/// Interpolates `{key}` placeholders in `template` with values from `args`.
///
/// Values are JSON-encoded; strings are inserted without quotes.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn interpolate(template: &str, args: &serde_json::Value) -> String {
    let mut result = template.to_string();
    if let serde_json::Value::Object(map) = args {
        for (key, value) in map {
            let placeholder = format!("{{{key}}}");
            let rendered = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&placeholder, &rendered);
        }
    }
    result
}

async fn execute_command(
    template: &str,
    working_dir: Option<&str>,
    args: serde_json::Value,
    cancel: CancellationToken,
) -> Result<String, ToolError> {
    let command = interpolate(template, &args);

    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(&command);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let child = cmd
        .spawn()
        .map_err(|e| ToolError::Io(std::io::Error::other(format!("spawn failed: {e}"))))?;

    let output = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            return Err(ToolError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            )));
        }
        result = child.wait_with_output() => {
            match result {
                Ok(o) => o,
                Err(err) => return Err(ToolError::Io(err)),
            }
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("stderr: ");
        result.push_str(&stderr);
    }

    if !output.status.success() {
        return Err(ToolError::Io(std::io::Error::other(format!(
            "exit code: {:?}",
            output.status.code()
        ))));
    }

    Ok(result)
}

async fn execute_http_post(
    url: &str,
    headers: &std::collections::BTreeMap<String, String>,
    args: serde_json::Value,
    cancel: CancellationToken,
) -> Result<String, ToolError> {
    let client = reqwest::Client::new();
    let mut request = client.post(url).json(&args);
    for (key, value) in headers {
        request = request.header(key, value);
    }

    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            return Err(ToolError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            )));
        }
        result = request.send() => result,
    };

    let response = response.map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;

    if !status.is_success() {
        return Err(ToolError::Io(std::io::Error::other(format!(
            "HTTP {}: {}",
            status, body
        ))));
    }

    Ok(body)
}

async fn execute_read_file(path: &str) -> Result<String, ToolError> {
    tokio::fs::read_to_string(path).await.map_err(ToolError::Io)
}
