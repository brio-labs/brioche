//! # Brioche Tool — Read File
//!
//! Reads the contents of a text file.
//!
//! ## Tool name
//! `read_file`
//!
//! ## Arguments
//! | Name | Type | Required | Description |
//! |------|------|----------|-------------|
//! | `path` | `string` | yes | Absolute or relative path to the file |
//!
//! ## Example
//! ```json
//! { "path": "/home/user/.bashrc" }
//! ```
//!
//! Refs: I-Shell-ToolResult-PassThrough

use brioche_shell_runtime::{SystemTool, ToolError, schema::object_schema};
use tokio_util::sync::CancellationToken;

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
        let content = tokio::fs::read_to_string(path).await?;
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_file_reads_existing_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        tokio::fs::write(temp.path(), "hello").await.unwrap();

        let tool = ReadFileTool;
        let args = serde_json::json!({ "path": temp.path().to_str().unwrap() });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn read_file_fails_on_missing_file() {
        let tool = ReadFileTool;
        let args = serde_json::json!({ "path": "/does/not/exist" });
        let result = tool.run(args, CancellationToken::new()).await;

        assert!(matches!(result, Err(ToolError::Io(_))));
    }

    #[tokio::test]
    async fn read_file_requires_path_arg() {
        let tool = ReadFileTool;
        let args = serde_json::json!({});
        let result = tool.run(args, CancellationToken::new()).await;

        assert!(matches!(result, Err(ToolError::InvalidArgs(_))));
    }

    #[tokio::test]
    async fn read_file_respects_cancellation() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        tokio::fs::write(temp.path(), "hello").await.unwrap();

        let tool = ReadFileTool;
        let args = serde_json::json!({ "path": temp.path().to_str().unwrap() });
        let cancel = CancellationToken::new();
        cancel.cancel();

        let result = tool.run(args, cancel).await;
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn schema_is_valid_json() {
        let tool = ReadFileTool;
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        assert!(schema.get("properties").is_some());
        assert!(schema.get("required").is_some());
    }
}
