//! Tool registry and executor.
//!
//! `SystemToolExecutor` implements the `ToolExecutor` trait from the Shell Runtime.
//! It delegates each call to the corresponding registered tool.
//!
//! Refs: I-Shell-ToolResult-PassThrough

use std::collections::BTreeMap;

use brioche_core::{ActiveToolCall, ToolOutcome, ToolResultDTO};
use brioche_shell_runtime::ToolExecutor;
use tokio_util::sync::CancellationToken;

/// Error emitted by a system tool.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Sandbox policy denied this command.
    #[error("sandbox denied: {0}")]
    SandboxDenied(String),
    /// I/O error during tool execution.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Tool arguments failed validation.
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    /// Requested tool is not registered.
    #[error("tool not found: {0}")]
    NotFound(String),
}

/// Trait for an individual system tool.
///
/// Each tool exposes its name, description, JSON schema, and
/// execution function.
///
/// Refs: I-Shell-ToolResult-PassThrough
#[async_trait::async_trait]
pub trait SystemTool: Send + Sync {
    /// Canonical tool name (unique identifier).
    fn name(&self) -> &'static str;
    /// Human-readable description for the LLM.
    fn description(&self) -> &'static str;
    /// JSON Schema of the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;
    /// Execute the tool with the given arguments.
    async fn run(
        &self,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<String, ToolError>;
}

/// System tool registry.
///
/// Maintains a `BTreeMap` name → tool, guaranteeing a deterministic
/// iteration order.
///
/// Refs: I-Eco-OrderedCollections
pub struct SystemToolExecutor {
    tools: BTreeMap<String, Box<dyn SystemTool>>,
}

impl SystemToolExecutor {
    /// Creates an empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    /// Registers a tool, replacing any existing tool with the same name.
    pub fn with_tool(mut self, tool: impl SystemTool + 'static) -> Self {
        let name = tool.name().to_string();
        self.tools.insert(name, Box::new(tool));
        self
    }

    /// Aggregates the JSON schemas of all registered tools.
    ///
    /// The format follows the OpenAI `tools` array specification:
    /// ```json
    /// [{"type": "function", "function": {"name": "...", "description": "...", "parameters": {...}}}]
    /// ```
    pub fn schema_json(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|tool| {
                let mut function = serde_json::Map::new();
                function.insert("name".into(), serde_json::Value::String(tool.name().into()));
                function.insert(
                    "description".into(),
                    serde_json::Value::String(tool.description().into()),
                );
                function.insert("parameters".into(), tool.parameters_schema());

                let mut obj = serde_json::Map::new();
                obj.insert("type".into(), serde_json::Value::String("function".into()));
                obj.insert("function".into(), serde_json::Value::Object(function));
                serde_json::Value::Object(obj)
            })
            .collect()
    }

    /// Executes a tool call.
    async fn run_tool(&self, call: &ActiveToolCall, cancel: CancellationToken) -> ToolResultDTO {
        let tool = match self.tools.get(&call.tool_name) {
            Some(t) => t,
            None => {
                return ToolResultDTO {
                    tool_id: call.tool_id.clone(),
                    tool_name: call.tool_name.clone(),
                    outcome: ToolOutcome::BusinessError(format!(
                        "tool '{}' not found",
                        call.tool_name
                    )),
                };
            }
        };

        let args = match serde_json::from_str::<serde_json::Value>(&call.arguments) {
            Ok(v) => v,
            Err(err) => {
                return ToolResultDTO {
                    tool_id: call.tool_id.clone(),
                    tool_name: call.tool_name.clone(),
                    outcome: ToolOutcome::BusinessError(format!("invalid JSON arguments: {}", err)),
                };
            }
        };

        match tool.run(args, cancel).await {
            Ok(output) => ToolResultDTO {
                tool_id: call.tool_id.clone(),
                tool_name: call.tool_name.clone(),
                outcome: ToolOutcome::Success(output),
            },
            Err(ToolError::SandboxDenied(reason)) => ToolResultDTO {
                tool_id: call.tool_id.clone(),
                tool_name: call.tool_name.clone(),
                outcome: ToolOutcome::BusinessError(format!("sandbox denied: {}", reason)),
            },
            Err(ToolError::Io(err)) => ToolResultDTO {
                tool_id: call.tool_id.clone(),
                tool_name: call.tool_name.clone(),
                outcome: ToolOutcome::SystemError(format!("io error: {}", err)),
            },
            Err(ToolError::InvalidArgs(reason)) => ToolResultDTO {
                tool_id: call.tool_id.clone(),
                tool_name: call.tool_name.clone(),
                outcome: ToolOutcome::BusinessError(format!("invalid arguments: {}", reason)),
            },
            Err(ToolError::NotFound(name)) => ToolResultDTO {
                tool_id: call.tool_id.clone(),
                tool_name: call.tool_name.clone(),
                outcome: ToolOutcome::BusinessError(format!("tool '{}' not found", name)),
            },
        }
    }
}

impl Default for SystemToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolExecutor for SystemToolExecutor {
    async fn execute(&self, call: &ActiveToolCall, cancel: CancellationToken) -> ToolResultDTO {
        self.run_tool(call, cancel).await
    }
}
