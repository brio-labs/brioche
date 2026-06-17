//! Execution adapter for user-defined tools.
//!
//! Wraps a [`UserToolDefinition`] so it implements the [`SystemTool`] trait
//! used by the shell runtime. User tools can execute shell commands, POST
//! JSON to URLs, or read files.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_tools_system::{SystemTool, ToolError};
use tokio_util::sync::CancellationToken;

use super::tool_provider::{ToolExecutor, UserToolDefinition};

/// A [`SystemTool`] implementation that delegates to a user-defined executor.
///
/// Refs: I-Shell-Runtime-OnlyIO
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

    let response = response
        .map_err(|err| ToolError::Io(std::io::Error::other(err.to_string())))?;
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
    tokio::fs::read_to_string(path)
        .await
        .map_err(ToolError::Io)
}
