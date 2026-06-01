//! # Brioche Tool — List Directory
//!
//! Lists the contents of a directory.
//!
//! ## Tool name
//! `list_dir`
//!
//! ## Arguments
//! | Name | Type | Required | Description |
//! |------|------|----------|-------------|
//! | `path` | `string` | yes | Absolute or relative path to the directory |
//!
//! ## Example
//! ```json
//! { "path": "/home/user" }
//! ```
//!
//! Refs: I-Shell-ToolResult-PassThrough

use brioche_shell_runtime::{SystemTool, ToolError, schema::object_schema};
use tokio_util::sync::CancellationToken;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_dir_lists_contents() {
        let temp = tempfile::tempdir().unwrap();
        tokio::fs::write(temp.path().join("a.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(temp.path().join("sub"))
            .await
            .unwrap();

        let tool = ListDirTool;
        let args = serde_json::json!({ "path": temp.path().to_str().unwrap() });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert!(result.contains("file a.txt"));
        assert!(result.contains("dir sub"));
    }

    #[tokio::test]
    async fn list_dir_fails_on_missing_dir() {
        let tool = ListDirTool;
        let args = serde_json::json!({ "path": "/does/not/exist" });
        let result = tool.run(args, CancellationToken::new()).await;

        assert!(matches!(result, Err(ToolError::Io(_))));
    }

    #[tokio::test]
    async fn list_dir_requires_path_arg() {
        let tool = ListDirTool;
        let args = serde_json::json!({});
        let result = tool.run(args, CancellationToken::new()).await;

        assert!(matches!(result, Err(ToolError::InvalidArgs(_))));
    }

    #[tokio::test]
    async fn list_dir_empty_dir() {
        let temp = tempfile::tempdir().unwrap();

        let tool = ListDirTool;
        let args = serde_json::json!({ "path": temp.path().to_str().unwrap() });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert_eq!(result, "");
    }

    #[test]
    fn schema_is_valid_json() {
        let tool = ListDirTool;
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
        assert!(schema.get("properties").is_some());
    }
}
