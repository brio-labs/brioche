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
use std::sync::RwLock;

use brioche_tools_system::{AllowList, ExecuteCommandTool, SystemTool, ToolError};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{ExtensionMetadata, PanelSlot};

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
    fn tools(&self) -> Result<Vec<ToolDescriptor>, String>;

    /// Returns user-defined tool definitions so they can be wired into the shell runtime.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn user_tools(&self) -> Result<Vec<UserToolDefinition>, String>;

    /// Enables or disables a tool by id.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String>;

    /// Adds a user-defined tool.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn add_user_tool(&self, tool: UserToolDefinition) -> Result<(), String>;

    /// Removes a user-defined tool.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn remove_user_tool(&self, id: &str) -> Result<(), String>;
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
    /// Execute a local shell command. Arguments are interpolated into the command string with `{arg_name}`.
    Command {
        /// Command template.
        command: String,
        /// Optional working directory.
        working_dir: Option<String>,
        /// Optional timeout in milliseconds.
        #[serde(default)]
        timeout_ms: Option<u64>,
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
#[derive(Debug, Default)]
pub struct ToolRegistry {
    user_tools: RwLock<Vec<UserToolDefinition>>,
    disabled: RwLock<Vec<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ToolRegistrySnapshot {
    user_tools: Vec<UserToolDefinition>,
    disabled: Vec<String>,
}
fn tool_schema(
    id: &str,
    name: &str,
    description: &str,
    category: &str,
    tags: &[&str],
    required: &[&str],
    properties: &[(&str, &str, &str)],
) -> ToolDescriptor {
    let mut props = serde_json::Map::new();
    for (k, t, d) in properties {
        let mut prop = serde_json::Map::new();
        prop.insert("type".into(), serde_json::Value::String((*t).into()));
        prop.insert("description".into(), serde_json::Value::String((*d).into()));
        props.insert((*k).into(), serde_json::Value::Object(prop));
    }
    let mut parameters = serde_json::Map::new();
    parameters.insert("type".into(), serde_json::Value::String("object".into()));
    parameters.insert("properties".into(), serde_json::Value::Object(props));
    if !required.is_empty() {
        parameters.insert(
            "required".into(),
            serde_json::Value::Array(required.iter().map(|r| (*r).into()).collect()),
        );
    }
    ToolDescriptor {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        parameters: serde_json::Value::Object(parameters),
        category: category.into(),
        tags: tags.iter().map(|t| (*t).into()).collect(),
        enabled: true,
        source: "built-in".into(),
    }
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
    /// Never panics. Returns Err if the configuration file cannot be read or parsed.
    pub fn load() -> Result<Self, String> {
        let path = registry_path();
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read tool registry: {e}"))?;
        let snapshot = serde_json::from_str::<ToolRegistrySnapshot>(&data)
            .map_err(|e| format!("Failed to parse tool registry: {e}"))?;
        Ok(Self {
            user_tools: RwLock::new(snapshot.user_tools),
            disabled: RwLock::new(snapshot.disabled),
        })
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
        let snapshot = ToolRegistrySnapshot {
            user_tools: self
                .user_tools
                .read()
                .map_err(|_| "Tool registry lock poisoned".to_string())?
                .clone(),
            disabled: self
                .disabled
                .read()
                .map_err(|_| "Tool registry lock poisoned".to_string())?
                .clone(),
        };
        let data = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| format!("Failed to serialize tools: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write tools: {e}"))
    }

    fn built_in_tools() -> Vec<ToolDescriptor> {
        vec![
            tool_schema(
                "read_file",
                "Read file",
                "Read the contents of a file",
                "filesystem",
                &["fs"],
                &["path"],
                &[("path", "string", "File path")],
            ),
            tool_schema(
                "write_file",
                "Write file",
                "Write content to a file",
                "filesystem",
                &["fs"],
                &["path", "content"],
                &[
                    ("path", "string", "File path"),
                    ("content", "string", "File content"),
                    ("append", "boolean", "Append to existing file"),
                ],
            ),
            tool_schema(
                "list_dir",
                "List directory",
                "List files in a directory",
                "filesystem",
                &["fs"],
                &["path"],
                &[("path", "string", "Directory path")],
            ),
            tool_schema(
                "execute_command",
                "Execute command",
                "Run a shell command",
                "system",
                &["shell"],
                &["command"],
                &[
                    ("command", "string", "Shell command"),
                    ("timeout_ms", "integer", "Timeout in milliseconds"),
                ],
            ),
            tool_schema(
                "fetch_url",
                "Fetch URL",
                "Fetch content from a URL",
                "web",
                &["http"],
                &["url"],
                &[("url", "string", "URL to fetch")],
            ),
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

    fn tools(&self) -> Result<Vec<ToolDescriptor>, String> {
        let mut tools = Self::built_in_tools();
        let disabled = self
            .disabled
            .read()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        let user_tools = self
            .user_tools
            .read()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        for user in user_tools.iter() {
            tools.push(ToolDescriptor {
                id: user.id.clone(),
                name: user.name.clone(),
                description: user.description.clone(),
                parameters: user.parameters.clone(),
                category: user.category.clone(),
                tags: user.tags.clone(),
                enabled: !disabled.contains(&user.id),
                source: "user-json".into(),
            });
        }
        for tool in &mut tools {
            if disabled.contains(&tool.id) {
                tool.enabled = false;
            }
        }
        Ok(tools)
    }

    fn user_tools(&self) -> Result<Vec<UserToolDefinition>, String> {
        let disabled = self
            .disabled
            .read()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        let user_tools = self
            .user_tools
            .read()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        Ok(user_tools
            .iter()
            .filter(|t| !disabled.contains(&t.id))
            .cloned()
            .collect())
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let mut disabled = self
            .disabled
            .write()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        if enabled {
            disabled.retain(|d| d != id);
        } else {
            let key = id.to_string();
            if !disabled.contains(&key) {
                disabled.push(key);
            }
        }
        drop(disabled);
        self.save()
    }

    fn add_user_tool(&self, tool: UserToolDefinition) -> Result<(), String> {
        let mut user_tools = self
            .user_tools
            .write()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        if user_tools.iter().any(|t| t.id == tool.id) {
            return Err(format!("Tool '{}' already exists", tool.id));
        }
        user_tools.push(tool);
        drop(user_tools);
        self.save()
    }

    fn remove_user_tool(&self, id: &str) -> Result<(), String> {
        let mut user_tools = self
            .user_tools
            .write()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        let len = user_tools.len();
        user_tools.retain(|t| t.id != id);
        if user_tools.len() == len {
            return Err(format!("Tool '{}' not found", id));
        }
        drop(user_tools);
        let mut disabled = self
            .disabled
            .write()
            .map_err(|_| "Tool registry lock poisoned".to_string())?;
        disabled.retain(|d| d != id);
        drop(disabled);
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
                timeout_ms,
            } => execute_command(command, working_dir.as_deref(), *timeout_ms, args, cancel).await,
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
    timeout_ms: Option<u64>,
    args: serde_json::Value,
    cancel: CancellationToken,
) -> Result<String, ToolError> {
    let command = interpolate(template, &args);

    // Defensive: reject shell metacharacters even before delegating to the
    // system tool, so injected commands like `ls; rm -rf /` are blocked.
    validate_no_shell_metacharacters(&command)?;

    let program = command
        .split_whitespace()
        .next()
        .ok_or_else(|| ToolError::InvalidArgs("command is empty".into()))?
        .to_string();

    let mut tool = ExecuteCommandTool::with_allow_list(AllowList::new().with_command(&program));
    if let Some(dir) = working_dir {
        tool = tool.with_default_cwd(dir);
    }

    let mut tool_args = serde_json::Map::new();
    tool_args.insert("command".into(), serde_json::Value::String(command));
    let timeout_ms = timeout_ms.map_or(30_000, |ms| ms);
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let result = tokio::time::timeout(
        timeout,
        tool.run(serde_json::Value::Object(tool_args), cancel),
    )
    .await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(ToolError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "command exceeded timeout",
        ))),
    }
}

/// Rejects commands that contain shell metacharacters.
///
/// Mirrors the validation used by `ExecuteCommandTool` so user-defined tools
/// cannot inject shell syntax through interpolated arguments.
fn validate_no_shell_metacharacters(command: &str) -> Result<(), ToolError> {
    for c in command.chars() {
        if matches!(
            c,
            ';' | '|' | '&' | '$' | '`' | '\n' | '\r' | '<' | '>' | '(' | ')' | '{' | '}'
        ) {
            return Err(ToolError::SandboxDenied(format!(
                "command contains shell metacharacter: {c}"
            )));
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_command_benign_succeeds()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"message":"hello"}"#)?;
        let output = execute_command(
            "echo {message}",
            None,
            Some(5_000),
            args,
            CancellationToken::new(),
        )
        .await?;
        assert!(
            output.contains("hello"),
            "expected 'hello' in command output"
        );
        Ok(())
    }

    #[tokio::test]
    async fn execute_command_rejects_shell_metacharacter_injection()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // The semicolon would let an attacker chain a second command; the
        // defense-in-depth validator must reject it before it reaches the shell.
        let args: serde_json::Value = serde_json::from_str(r#"{"message":"hello; rm -rf /"}"#)?;
        let result = execute_command(
            "echo {message}",
            None,
            Some(5_000),
            args,
            CancellationToken::new(),
        )
        .await;
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => return Err("expected injection to be rejected".into()),
        };
        assert!(
            err.contains("shell metacharacter"),
            "expected metacharacter error, got {err}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn execute_command_enforces_timeout()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"duration":"2"}"#)?;
        let result = execute_command(
            "sleep {duration}",
            None,
            Some(50),
            args,
            CancellationToken::new(),
        )
        .await;
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => return Err("expected timeout error".into()),
        };
        assert!(err.contains("timeout"), "expected timeout error, got {err}");
        Ok(())
    }

    #[test]
    fn validate_no_shell_metacharacters_blocks_common_injection_chars()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for bad in [";", "|", "&", "$", "`", "<", ">", "(", ")", "{", "}"] {
            let cmd = format!("echo {bad}");
            let err = match validate_no_shell_metacharacters(&cmd) {
                Err(e) => e,
                Ok(()) => return Err(format!("expected rejection for {bad:?}").into()),
            };
            assert!(err.to_string().contains("shell metacharacter"));
        }
        Ok(())
    }

    #[test]
    fn validate_no_shell_metacharacters_allows_safe_command()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        validate_no_shell_metacharacters("echo hello world")?;
        validate_no_shell_metacharacters("ls -la /tmp")?;
        Ok(())
    }
}
