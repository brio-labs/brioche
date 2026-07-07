//! Sandboxed command execution tool.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::registry::{AllowList, ConfirmHandler, SandboxPolicy, SystemTool, ToolError};

/// Executes a command with a sandbox policy.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct ExecuteCommandTool {
    policy: SandboxPolicy,
    confirm_handler: Option<ConfirmHandler>,
    default_cwd: Option<String>,
}

impl ExecuteCommandTool {
    /// Creates a new command tool with default sandbox policy.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new() -> Self {
        Self {
            policy: SandboxPolicy::default(),
            confirm_handler: None,
            default_cwd: None,
        }
    }

    /// Sets the sandbox policy explicitly.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Sets a default working directory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn with_default_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.default_cwd = Some(cwd.into());
        self
    }

    /// Creates the tool with an explicit allow-list.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
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
        "Execute a command. Only allowed commands are permitted by default.".into()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        let mut props = serde_json::Map::new();

        let mut command = serde_json::Map::new();
        command.insert("type".into(), serde_json::Value::String("string".into()));
        command.insert(
            "description".into(),
            serde_json::Value::String("The command to execute".into()),
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

        validate_no_shell_metacharacters(command)?;
        let argv = split_command(command)?;
        let program = argv
            .first()
            .ok_or_else(|| ToolError::InvalidArgs("command is empty".into()))?;

        match &self.policy {
            SandboxPolicy::Permissive => {
                tracing::warn!(command, program, "executing command in permissive sandbox");
            }
            SandboxPolicy::AllowList(list) => {
                if !list.is_allowed(program) {
                    if let Some(handler) = &self.confirm_handler {
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
                if let Some(handler) = &self.confirm_handler {
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

        let mut cmd = tokio::process::Command::new(program);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        for arg in argv.iter().skip(1) {
            cmd.arg(arg);
        }

        if let Some(cwd) = args["cwd"].as_str() {
            cmd.current_dir(cwd);
        } else if let Some(default_cwd) = &self.default_cwd {
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

/// Rejects commands that contain shell metacharacters.
///
/// Because `ExecuteCommandTool` no longer invokes `/bin/sh -c`,
/// metacharacters are not interpreted; rejecting them defends against
/// accidental or malicious shell syntax in user input.
fn validate_no_shell_metacharacters(command: &str) -> Result<(), ToolError> {
    for c in command.chars() {
        if matches!(
            c,
            ';' | '|' | '&' | '$' | '`' | '\n' | '\r' | '<' | '>' | '(' | ')' | '{' | '}'
        ) {
            return Err(ToolError::InvalidArgs(format!(
                "command contains shell metacharacter: {c}"
            )));
        }
    }
    Ok(())
}

/// Splits a command string into an argv vector.
///
/// Supports single and double quotes and backslash escaping outside
/// single quotes. Returns `ToolError::InvalidArgs` on unclosed quotes
/// or a trailing backslash.
fn split_command(command: &str) -> Result<Vec<String>, ToolError> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut chars = command.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_token = false;

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
                in_token = true;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                in_token = true;
            }
            '\\' if !in_single_quote => match chars.next() {
                Some(next) => {
                    current.push(next);
                    in_token = true;
                }
                None => {
                    return Err(ToolError::InvalidArgs(
                        "command ends with a backslash".into(),
                    ));
                }
            },
            c if c.is_whitespace() && !in_single_quote && !in_double_quote => {
                if in_token {
                    argv.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            c => {
                current.push(c);
                in_token = true;
            }
        }
    }

    if in_single_quote || in_double_quote {
        return Err(ToolError::InvalidArgs(
            "command has an unclosed quote".into(),
        ));
    }

    if in_token {
        argv.push(current);
    }

    if argv.is_empty() {
        return Err(ToolError::InvalidArgs("command is empty".into()));
    }

    Ok(argv)
}
