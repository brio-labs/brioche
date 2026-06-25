//! Tool registry, executor, and sandbox policy.
//!
//! `SystemToolExecutor` implements the `ToolExecutor` trait from the Shell Runtime.
//! It delegates each call to the corresponding registered tool.
//!
//! Refs: I-Shell-ToolResult-PassThrough

use std::collections::{BTreeMap, BTreeSet};

use brioche_core::{ActiveToolCall, ToolOutcome, ToolResultDTO};
use brioche_shell_runtime::ToolExecutor;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Sandbox policy (merged from sandbox.rs)
// ---------------------------------------------------------------------------

/// Interactive confirmation handler for shell commands.
///
/// Returns `true` if the user confirms execution.
/// The handler may block on stdin; it is called inside
/// `tokio::task::spawn_blocking` by the tool.
pub type ConfirmHandler = std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Sandbox policy for shell commands.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Clone, Debug)]
pub enum SandboxPolicy {
    /// Any command is allowed (dangerous mode, requires confirmation).
    Permissive,
    /// Only commands in the allow-list are allowed.
    /// Others trigger an interactive confirmation if a
    /// `ConfirmHandler` is configured on the tool, otherwise a
    /// `ToolError::SandboxDenied` error.
    AllowList(AllowList),
    /// Every command requires interactive confirmation.
    /// The handler must be configured on the tool; otherwise returns
    /// an error in headless mode.
    Interactive,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::AllowList(AllowList::default())
    }
}

/// Explicit list of allowed commands.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Clone, Debug)]
pub struct AllowList {
    commands: BTreeSet<String>,
}

impl AllowList {
    /// Creates an empty allow-list.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new() -> Self {
        Self {
            commands: BTreeSet::new(),
        }
    }

    /// Adds a command to the allow-list.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_command(mut self, cmd: &str) -> Self {
        self.commands.insert(cmd.to_string());
        self
    }

    /// Checks whether a command is in the allow-list.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn is_allowed(&self, command: &str) -> bool {
        let first_word = command.split_whitespace().next().map_or("", |s| s).trim();
        self.commands.contains(first_word)
    }
}

impl Default for AllowList {
    /// Default allow-list contains only read-only, safe commands.
    ///
    /// Compilers and version-control tools (`cargo`, `rustc`, `git`)
    /// and broad filesystem scanners (`find`) are opt-in via
    /// `with_command` to reduce default blast radius.
    fn default() -> Self {
        Self::new()
            .with_command("ls")
            .with_command("cat")
            .with_command("grep")
            .with_command("pwd")
            .with_command("echo")
            .with_command("head")
            .with_command("tail")
            .with_command("wc")
    }
}

// ---------------------------------------------------------------------------
// Tool registry and executor
// ---------------------------------------------------------------------------

/// Error emitted by a system tool.
/// Refs: docs/SPECS.md §Book III-C
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
    fn name(&self) -> String;
    /// Human-readable description for the LLM.
    fn description(&self) -> String;
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
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    /// Registers a tool, replacing any existing tool with the same name.
    /// Refs: docs/SPECS.md §Book III-C
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
    /// Refs: docs/SPECS.md §Book III-C
    pub fn schema_json(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|tool| {
                let mut function = serde_json::Map::new();
                function.insert("name".into(), serde_json::Value::String(tool.name()));
                function.insert(
                    "description".into(),
                    serde_json::Value::String(tool.description()),
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
