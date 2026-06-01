//! File system tools.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use crate::registry::{SystemTool, ToolError};
use tokio_util::sync::CancellationToken;

fn object_schema(required: &[&str], properties: &[(&str, &str)]) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    for (name, description) in properties {
        let mut p = serde_json::Map::new();
        p.insert("type".into(), serde_json::Value::String("string".into()));
        p.insert(
            "description".into(),
            serde_json::Value::String((*description).into()),
        );
        props.insert((*name).into(), serde_json::Value::Object(p));
    }
    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), serde_json::Value::String("object".into()));
    schema.insert("properties".into(), serde_json::Value::Object(props));
    schema.insert(
        "required".into(),
        serde_json::Value::Array(
            required
                .iter()
                .map(|s| serde_json::Value::String((*s).into()))
                .collect(),
        ),
    );
    serde_json::Value::Object(schema)
}

/// Reads the contents of a text file.
pub struct ReadFileTool;

#[async_trait::async_trait]
impl SystemTool for ReadFileTool {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read the contents of a text file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        object_schema(
            &["path"],
            &[("path", "Absolute or relative path to the file")],
        )
    }

    async fn run(
        &self,
        args: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let content = tokio::fs::read_to_string(path).await?;
        Ok(content)
    }
}

/// Writes content to a text file.
pub struct WriteFileTool;

#[async_trait::async_trait]
impl SystemTool for WriteFileTool {
    fn name(&self) -> &'static str {
        "write_file"
    }

    fn description(&self) -> &'static str {
        "Write content to a text file. Creates the file if it does not exist."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        object_schema(
            &["path", "content"],
            &[
                ("path", "Absolute or relative path to the file"),
                ("content", "Text content to write"),
            ],
        )
    }

    async fn run(
        &self,
        args: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let content = args["content"].as_str().unwrap_or("");
        tokio::fs::write(path, content).await?;
        Ok(format!("written {} bytes to {}", content.len(), path))
    }
}

/// Lists the contents of a directory.
pub struct ListDirTool;

#[async_trait::async_trait]
impl SystemTool for ListDirTool {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "List the contents of a directory."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        object_schema(
            &["path"],
            &[("path", "Absolute or relative path to the directory")],
        )
    }

    async fn run(
        &self,
        args: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let mut entries = tokio::fs::read_dir(path).await?;
        let mut lines = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let meta = entry.metadata().await?;
            let kind = if meta.is_dir() { "dir" } else { "file" };
            lines.push(format!("{} {}", kind, entry.file_name().to_string_lossy()));
        }
        Ok(lines.join("\n"))
    }
}
