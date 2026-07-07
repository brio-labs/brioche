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

use super::{ExtensionMetadata, PanelSlot, PersistenceError};

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
    fn tools(&self) -> Result<Vec<ToolDescriptor>, PersistenceError>;

    /// Returns user-defined tool definitions so they can be wired into the shell runtime.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn user_tools(&self) -> Result<Vec<UserToolDefinition>, PersistenceError>;

    /// Enables or disables a tool by id.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), PersistenceError>;

    /// Adds a user-defined tool.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn add_user_tool(&self, tool: UserToolDefinition) -> Result<(), PersistenceError>;

    /// Removes a user-defined tool.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn remove_user_tool(&self, id: &str) -> Result<(), PersistenceError>;
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
    pub fn load() -> Result<Self, PersistenceError> {
        let path = registry_path();
        let data = std::fs::read_to_string(&path)?;
        let snapshot = serde_json::from_str::<ToolRegistrySnapshot>(&data)?;
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
    pub fn save(&self) -> Result<(), PersistenceError> {
        let path = registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let snapshot = ToolRegistrySnapshot {
            user_tools: self.user_tools.read()?.clone(),
            disabled: self.disabled.read()?.clone(),
        };
        let data = serde_json::to_string_pretty(&snapshot).map_err(PersistenceError::Json)?;
        std::fs::write(&path, data).map_err(PersistenceError::Io)
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

    fn tools(&self) -> Result<Vec<ToolDescriptor>, PersistenceError> {
        let mut tools = Self::built_in_tools();
        let disabled = self.disabled.read()?;
        let user_tools = self.user_tools.read()?;
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

    fn user_tools(&self) -> Result<Vec<UserToolDefinition>, PersistenceError> {
        let disabled = self.disabled.read()?;
        let user_tools = self.user_tools.read()?;
        Ok(user_tools
            .iter()
            .filter(|t| !disabled.contains(&t.id))
            .cloned()
            .collect())
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), PersistenceError> {
        let mut disabled = self.disabled.write()?;
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

    fn add_user_tool(&self, tool: UserToolDefinition) -> Result<(), PersistenceError> {
        let mut user_tools = self.user_tools.write()?;
        if user_tools.iter().any(|t| t.id == tool.id) {
            return Err(PersistenceError::AlreadyExists(tool.id.clone()));
        }
        user_tools.push(tool);
        drop(user_tools);
        self.save()
    }

    fn remove_user_tool(&self, id: &str) -> Result<(), PersistenceError> {
        let mut user_tools = self.user_tools.write()?;
        let len = user_tools.len();
        user_tools.retain(|t| t.id != id);
        if user_tools.len() == len {
            return Err(PersistenceError::NotFound(format!("Tool '{id}'")));
        }
        drop(user_tools);
        let mut disabled = self.disabled.write()?;
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
/// Values are JSON-encoded; strings are inserted without quotes. Placeholder
/// keys must match `^[a-zA-Z0-9_]+$` and all braces in the template must be
/// balanced. Malformed placeholders are returned as
/// [`ToolError::InvalidArgs`].
///
/// Refs: I-Shell-Runtime-OnlyIO
fn interpolate(template: &str, args: &serde_json::Value) -> Result<String, ToolError> {
    let mut result = template.to_string();

    validate_template_placeholders(template)?;

    if let serde_json::Value::Object(map) = args {
        for (key, value) in map {
            validate_placeholder_key(key)?;
            let placeholder = format!("{{{key}}}");
            let rendered = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&placeholder, &rendered);
        }
    }
    Ok(result)
}

/// Validates that all `{key}` placeholders in `template` are balanced and
/// syntactically valid.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn validate_template_placeholders(template: &str) -> Result<(), ToolError> {
    let mut chars = template.chars().enumerate();
    while let Some((i, c)) = chars.next() {
        if c == '{' {
            let start = i;
            let mut key = String::new();
            let mut found_close = false;
            for (j, c2) in chars.by_ref() {
                if c2 == '}' {
                    found_close = true;
                    break;
                }
                if c2 == '{' {
                    return Err(ToolError::InvalidArgs(format!(
                        "nested opening brace at position {j} in placeholder starting at position {start}"
                    )));
                }
                key.push(c2);
            }
            if !found_close {
                return Err(ToolError::InvalidArgs(format!(
                    "unclosed placeholder starting at position {start}"
                )));
            }
            validate_placeholder_key(&key)?;
        } else if c == '}' {
            return Err(ToolError::InvalidArgs(format!(
                "unbalanced closing brace at position {i}"
            )));
        }
    }
    Ok(())
}

/// Validates that `key` is a legal placeholder key.
///
/// Legal keys match the regex `^[a-zA-Z0-9_]+$`.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn validate_placeholder_key(key: &str) -> Result<(), ToolError> {
    if key.is_empty() {
        return Err(ToolError::InvalidArgs("empty placeholder key".into()));
    }
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ToolError::InvalidArgs(format!(
            "invalid placeholder key: {key}"
        )));
    }
    Ok(())
}

async fn execute_command(
    template: &str,
    working_dir: Option<&str>,
    timeout_ms: Option<u64>,
    args: serde_json::Value,
    cancel: CancellationToken,
) -> Result<String, ToolError> {
    // Determine the allowed program from the trusted template, not from the
    // interpolated command, so a placeholder cannot change which binary is run.
    let program = template
        .split_whitespace()
        .next()
        .ok_or_else(|| ToolError::InvalidArgs("command template is empty".into()))?
        .to_string();

    // Validate argument values before interpolation so placeholders cannot
    // introduce shell metacharacters.
    validate_interpolated_values(&args)?;

    let command = interpolate(template, &args)?;

    // Defensive: reject shell metacharacters in the final command so a
    // malformed template cannot bypass the sandbox.
    validate_no_shell_metacharacters(&command)?;

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

/// Validates that every argument value to be interpolated is free of shell
/// metacharacters.
///
/// This check runs before interpolation so a placeholder value like
/// `hello; rm -rf /` is rejected before it can be inserted into the command.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn validate_interpolated_values(args: &serde_json::Value) -> Result<(), ToolError> {
    if let serde_json::Value::Object(map) = args {
        for (key, value) in map {
            let rendered = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            validate_value_no_shell_metacharacters(key, &rendered)?;
        }
    }
    Ok(())
}

/// Rejects a single argument value if it contains shell metacharacters.
///
/// Mirrors the validation used by `ExecuteCommandTool` so user-defined tools
/// cannot inject shell syntax through interpolated arguments.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn validate_value_no_shell_metacharacters(key: &str, value: &str) -> Result<(), ToolError> {
    for c in value.chars() {
        if matches!(
            c,
            ';' | '|' | '&' | '$' | '`' | '\n' | '\r' | '<' | '>' | '(' | ')' | '{' | '}'
        ) {
            return Err(ToolError::SandboxDenied(format!(
                "argument '{key}' contains shell metacharacter: {c}"
            )));
        }
    }
    Ok(())
}

/// Rejects commands that contain shell metacharacters.
///
/// Mirrors the validation used by `ExecuteCommandTool` so user-defined tools
/// cannot inject shell syntax through interpolated arguments.
///
/// Refs: I-Shell-Runtime-OnlyIO
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
    brioche_tools_system::post_json(url, headers, args, cancel).await
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
        // value validator must reject it before interpolation reaches the shell.
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
            err.contains("argument 'message' contains shell metacharacter"),
            "expected argument-specific metacharacter error, got {err}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn execute_command_honors_allow_list()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // The allowed program is derived from the trusted template, not from
        // an interpolated value, so a placeholder in the first position is
        // not a backdoor to run arbitrary binaries.
        let args: serde_json::Value =
            serde_json::from_str(r#"{"program":"cat","arg":"/etc/passwd"}"#)?;
        let result = execute_command(
            "{program} {arg}",
            None,
            Some(5_000),
            args,
            CancellationToken::new(),
        )
        .await;
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => return Err("expected allow-list denial".into()),
        };
        assert!(
            err.contains("not in the allow-list"),
            "expected allow-list error, got {err}"
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

    #[test]
    fn interpolate_replaces_valid_placeholders()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"message":"hello","count":42}"#)?;
        let result = interpolate("echo {message} {count}", &args)?;
        assert_eq!(result, "echo hello 42");
        Ok(())
    }

    #[test]
    fn interpolate_rejects_key_containing_closing_brace()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"bad}":"x"}"#)?;
        let err = match interpolate("echo {bad}", &args) {
            Ok(_) => return Err("expected interpolation error for key containing }".into()),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("invalid placeholder key"),
            "expected invalid placeholder key error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn interpolate_rejects_unbalanced_opening_brace()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"message":"hello"}"#)?;
        let err = match interpolate("echo {message", &args) {
            Ok(_) => return Err("expected interpolation error for unbalanced opening brace".into()),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("unclosed placeholder"),
            "expected unclosed placeholder error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn interpolate_rejects_unbalanced_closing_brace()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"message":"hello"}"#)?;
        let err = match interpolate("echo message}", &args) {
            Ok(_) => return Err("expected interpolation error for unbalanced closing brace".into()),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("unbalanced closing brace"),
            "expected unbalanced closing brace error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn interpolate_rejects_invalid_key_characters()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"bad-key":"x"}"#)?;
        let err = match interpolate("echo {bad-key}", &args) {
            Ok(_) => return Err("expected interpolation error for invalid key characters".into()),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("invalid placeholder key"),
            "expected invalid placeholder key error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn interpolate_rejects_nested_braces() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"message":"hello"}"#)?;
        let err = match interpolate("echo {{message}}", &args) {
            Ok(_) => return Err("expected interpolation error for nested braces".into()),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("nested opening brace"),
            "expected nested brace error, got {err}"
        );
        Ok(())
    }

    #[test]
    fn interpolate_rejects_nested_placeholder()
    -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let args: serde_json::Value = serde_json::from_str(r#"{"a":"x","b":"y"}"#)?;
        let err = match interpolate("echo {a{b}}", &args) {
            Ok(_) => return Err("expected interpolation error for nested placeholder".into()),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("nested opening brace"),
            "expected nested brace error, got {err}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn http_post_rejects_localhost() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut payload = serde_json::Map::new();
        payload.insert("message".into(), serde_json::Value::String("hello".into()));
        let result = execute_http_post(
            "http://localhost:8080/hook",
            &BTreeMap::new(),
            serde_json::Value::Object(payload),
            CancellationToken::new(),
        )
        .await;
        let err = match result {
            Err(err) => err,
            Ok(_) => return Err("localhost HTTP POST must be blocked".into()),
        };
        assert!(
            err.to_string().contains("localhost"),
            "expected localhost denial, got {err}"
        );
        Ok(())
    }
}
