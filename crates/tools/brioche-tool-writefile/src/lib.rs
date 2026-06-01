//! # Brioche Tool — Write File
//!
//! Writes content to a text file.
//!
//! ## Tool name
//! `write_file`
//!
//! ## Arguments
//! | Name | Type | Required | Description |
//! |------|------|----------|-------------|
//! | `path` | `string` | yes | Absolute or relative path to the file |
//! | `content` | `string` | yes | Text content to write |
//!
//! ## Example
//! ```json
//! { "path": "/tmp/note.txt", "content": "hello world" }
//! ```
//!
//! Refs: I-Shell-ToolResult-PassThrough

use brioche_shell_runtime::{SystemTool, ToolError, schema::object_schema};
use tokio_util::sync::CancellationToken;

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
        cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        if cancel.is_cancelled() {
            return Err(ToolError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            )));
        }
        let path = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let content = args["content"].as_str().unwrap_or("");
        tokio::fs::write(path, content).await?;
        Ok(format!("written {} bytes to {}", content.len(), path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_file_writes_content() {
        let temp = tempfile::NamedTempFile::new().unwrap();

        let tool = WriteFileTool;
        let args = serde_json::json!({
            "path": temp.path().to_str().unwrap(),
            "content": "hello world"
        });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert!(result.contains("11 bytes"));
        let read = tokio::fs::read_to_string(temp.path()).await.unwrap();
        assert_eq!(read, "hello world");
    }

    #[tokio::test]
    async fn write_file_requires_path_arg() {
        let tool = WriteFileTool;
        let args = serde_json::json!({ "content": "hello" });
        let result = tool.run(args, CancellationToken::new()).await;

        assert!(matches!(result, Err(ToolError::InvalidArgs(_))));
    }

    #[tokio::test]
    async fn write_file_allows_empty_content() {
        let temp = tempfile::NamedTempFile::new().unwrap();

        let tool = WriteFileTool;
        let args = serde_json::json!({
            "path": temp.path().to_str().unwrap(),
            "content": ""
        });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert!(result.contains("0 bytes"));
    }

    #[tokio::test]
    async fn write_file_creates_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("new_file.txt");

        let tool = WriteFileTool;
        let args = serde_json::json!({
            "path": path.to_str().unwrap(),
            "content": "created"
        });
        tool.run(args, CancellationToken::new()).await.unwrap();

        assert!(path.exists());
        let read = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(read, "created");
    }

    #[test]
    fn schema_is_valid_json() {
        let tool = WriteFileTool;
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v == "path"));
        assert!(required.iter().any(|v| v == "content"));
    }
}
