//! # Brioche Tool — Execute Command
//!
//! Executes a shell command with a configurable sandbox policy.
//!
//! ## Tool name
//! `execute_command`
//!
//! ## Arguments
//! | Name | Type | Required | Description |
//! |------|------|----------|-------------|
//! | `command` | `string` | yes | The shell command to execute |
//! | `cwd` | `string` | no | Working directory (optional) |
//!
//! ## Safety
//! - Only allowed commands are permitted by default (allow-list).
//! - Interactive confirmation can be configured for unknown commands.
//! - The agent is responsible for configuring the sandbox policy.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_shell_runtime::{AllowList, ConfirmHandler, SandboxPolicy, SystemTool, ToolError};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Executes a shell command with a sandbox policy.
#[derive(Clone)]
pub struct ExecuteCommandTool {
    policy: SandboxPolicy,
    confirm_handler: Option<ConfirmHandler>,
}

impl std::fmt::Debug for ExecuteCommandTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecuteCommandTool")
            .field("policy", &self.policy)
            .field("confirm_handler", &self.confirm_handler.is_some())
            .finish()
    }
}

impl ExecuteCommandTool {
    pub fn new() -> Self {
        Self {
            policy: SandboxPolicy::default(),
            confirm_handler: None,
        }
    }

    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub fn with_allow_list(list: AllowList) -> Self {
        Self {
            policy: SandboxPolicy::AllowList(list),
            confirm_handler: None,
        }
    }

    /// Sets an interactive confirmation handler.
    ///
    /// When a command is outside the allow-list (or in
    /// `Interactive` mode), the handler is called inside
    /// `spawn_blocking` to ask the user for confirmation.
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
    fn name(&self) -> &'static str {
        "execute_command"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command. Only allowed commands are permitted by default."
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
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if let Some(cwd) = args["cwd"].as_str() {
            cmd.current_dir(cwd);
        }

        if cancel.is_cancelled() {
            return Err(ToolError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            )));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_allowed_command() {
        let tool = ExecuteCommandTool::with_allow_list(AllowList::new().with_command("echo"));
        let args = serde_json::json!({ "command": "echo hello" });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn execute_denied_command() {
        let tool = ExecuteCommandTool::with_allow_list(AllowList::new());
        let args = serde_json::json!({ "command": "echo hello" });
        let result = tool.run(args, CancellationToken::new()).await;

        assert!(matches!(result, Err(ToolError::SandboxDenied(_))));
    }

    #[tokio::test]
    async fn execute_requires_command_arg() {
        let tool = ExecuteCommandTool::new();
        let args = serde_json::json!({});
        let result = tool.run(args, CancellationToken::new()).await;

        assert!(matches!(result, Err(ToolError::InvalidArgs(_))));
    }

    #[tokio::test]
    async fn execute_respects_cancellation() {
        let tool = ExecuteCommandTool::with_allow_list(AllowList::new().with_command("echo"));
        let args = serde_json::json!({ "command": "echo hello" });
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = tool.run(args, cancel).await;
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn schema_is_valid_json() {
        let tool = ExecuteCommandTool::new();
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v == "command"));
    }
}
