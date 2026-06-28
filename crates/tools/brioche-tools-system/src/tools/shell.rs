//! Sandboxed shell command execution tool.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::registry::{AllowList, ConfirmHandler, SandboxPolicy, SystemTool, ToolError};

/// Executes a shell command with a sandbox policy.
/// Refs: docs/SPECS.md §Book III-C
pub struct ExecuteCommandTool {
    policy: SandboxPolicy,
    confirm_handler: Option<ConfirmHandler>,
    default_cwd: Option<String>,
}

impl ExecuteCommandTool {
    /// Creates a new shell command tool with default sandbox policy.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new() -> Self {
        Self {
            policy: SandboxPolicy::default(),
            confirm_handler: None,
            default_cwd: None,
        }
    }

    /// Sets the sandbox policy explicitly.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Sets a default working directory.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_default_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.default_cwd = Some(cwd.into());
        self
    }

    /// Creates the tool with an explicit allow-list.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_allow_list(list: AllowList) -> Self {
        Self {
            policy: SandboxPolicy::AllowList(list),
            confirm_handler: None,
            default_cwd: None,
        }
    }

    /// Sets an interactive confirmation handler.
    ///
    /// When a command is outside the allow-list (or in
    /// `Interactive` mode), the handler is called inside
    /// `spawn_blocking` to ask the user for confirmation.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_confirm_handler(mut self, handler: ConfirmHandler) -> Self {
        self.confirm_handler = Some(handler);
        self
    }
}

impl Default for ExecuteCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SystemTool for ExecuteCommandTool {
    fn name(&self) -> String {
        "execute_command".into()
    }

    fn description(&self) -> String {
        "Execute a shell command. Only allowed commands are permitted by default.".into()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        let mut props = serde_json::Map::new();

        let mut command = serde_json::Map::new();
        command.insert("type".into(), serde_json::Value::String("string".into()));
        command.insert(
            "description".into(),
            serde_json::Value::String("The shell command to execute".into()),
        );
        props.insert("command".into(), serde_json::Value::Object(command));

        let mut cwd = serde_json::Map::new();
        cwd.insert("type".into(), serde_json::Value::String("string".into()));
        cwd.insert(
            "description".into(),
            serde_json::Value::String("Working directory (optional)".into()),
        );
        props.insert("cwd".into(), serde_json::Value::Object(cwd));

        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), serde_json::Value::String("object".into()));
        schema.insert("properties".into(), serde_json::Value::Object(props));
        schema.insert(
            "required".into(),
            serde_json::Value::Array(vec![serde_json::Value::String("command".into())]),
        );
        serde_json::Value::Object(schema)
    }

    async fn run(
        &self,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'command'".into()))?;

        match &self.policy {
            SandboxPolicy::Permissive => {
                tracing::warn!(command, "executing command in permissive sandbox");
                if let Some(ref handler) = self.confirm_handler {
                    let cmd = command.to_string();
                    let handler = Arc::clone(handler);
                    let confirmed = tokio::task::spawn_blocking(move || handler(&cmd))
                        .await
                        .map_err(|e| {
                            ToolError::Io(std::io::Error::other(format!(
                                "confirm task failed: {e}"
                            )))
                        })?;
                    if !confirmed {
                        return Err(ToolError::SandboxDenied(format!(
                            "command '{command}' was denied by user"
                        )));
                    }
                } else {
                    return Err(ToolError::SandboxDenied(format!(
                        "command '{command}' requires confirmation in permissive sandbox"
                    )));
                }
            }
            SandboxPolicy::AllowList(list) => {
                if !list.is_allowed(command) {
                    if let Some(ref handler) = self.confirm_handler {
                        let cmd = command.to_string();
                        let handler = Arc::clone(handler);
                        let confirmed = tokio::task::spawn_blocking(move || handler(&cmd))
                            .await
                            .map_err(|e| {
                                ToolError::Io(std::io::Error::other(format!(
                                    "confirm task failed: {e}"
                                )))
                            })?;
                        if !confirmed {
                            return Err(ToolError::SandboxDenied(format!(
                                "command '{command}' was denied by user"
                            )));
                        }
                    } else {
                        return Err(ToolError::SandboxDenied(format!(
                            "command '{command}' is not in the allow-list"
                        )));
                    }
                }
            }
            SandboxPolicy::Interactive => {
                if let Some(ref handler) = self.confirm_handler {
                    let cmd = command.to_string();
                    let handler = Arc::clone(handler);
                    let confirmed = tokio::task::spawn_blocking(move || handler(&cmd))
                        .await
                        .map_err(|e| {
                            ToolError::Io(std::io::Error::other(format!(
                                "confirm task failed: {e}"
                            )))
                        })?;
                    if !confirmed {
                        return Err(ToolError::SandboxDenied(format!(
                            "command '{command}' was denied by user"
                        )));
                    }
                } else {
                    return Err(ToolError::SandboxDenied(
                        "interactive confirmation not available in headless mode".into(),
                    ));
                }
            }
        }

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if let Some(cwd) = args["cwd"].as_str() {
            cmd.current_dir(cwd);
        } else if let Some(ref default_cwd) = self.default_cwd {
            cmd.current_dir(default_cwd);
        }

        let child = cmd.spawn()?;

        let output = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                return Err(ToolError::Io(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "cancelled",
                )));
            }
            result = async move {
                let child = child;
                child.wait_with_output().await
            } => {
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
}
