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
//! | `append` | `boolean` | no | If true, append instead of overwrite |
//!
//! ## Example
//! ```json
//! { "path": "/tmp/note.txt", "content": "hello world" }
//! ```
//!
//! Refs: I-Shell-ToolResult-PassThrough

use brioche_shell_runtime::{SystemTool, ToolError};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

/// Expand a leading `~` to the user's home directory.
///
/// Models frequently emit paths like `~/Desktop/file.html`.
/// This is a mechanical convenience, not policy.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{}/{}", home.trim_end_matches('/'), rest);
    }
    path.into()
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
        let mut props = serde_json::Map::new();
        let mut path_p = serde_json::Map::new();
        path_p.insert("type".into(), "string".into());
        path_p.insert(
            "description".into(),
            "Absolute or relative path to the file".into(),
        );
        props.insert("path".into(), serde_json::Value::Object(path_p));

        let mut content_p = serde_json::Map::new();
        content_p.insert("type".into(), "string".into());
        content_p.insert("description".into(), "Text content to write".into());
        props.insert("content".into(), serde_json::Value::Object(content_p));

        let mut append_p = serde_json::Map::new();
        append_p.insert("type".into(), "boolean".into());
        append_p.insert(
            "description".into(),
            "If true, append to the file instead of overwriting".into(),
        );
        props.insert("append".into(), serde_json::Value::Object(append_p));

        let mut schema = serde_json::Map::new();
        schema.insert("type".into(), "object".into());
        schema.insert("properties".into(), serde_json::Value::Object(props));
        schema.insert(
            "required".into(),
            serde_json::Value::Array(vec!["path".into(), "content".into()]),
        );
        serde_json::Value::Object(schema)
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
        let path_raw = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let path = expand_tilde(path_raw);
        let content = args["content"].as_str().unwrap_or("");
        let append = args["append"].as_bool().unwrap_or(false);

        // Create parent directories if they don't exist.
        if let Some(parent) = std::path::Path::new(&path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        if append {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await?;
            file.write_all(content.as_bytes()).await?;
            file.flush().await?;
            Ok(format!("appended {} bytes to {}", content.len(), path))
        } else {
            tokio::fs::write(&path, content).await?;
            Ok(format!("written {} bytes to {}", content.len(), path))
        }
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
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("append"));
        assert_eq!(
            props["append"].get("type").unwrap().as_str().unwrap(),
            "boolean"
        );
    }

    #[tokio::test]
    async fn write_file_appends_content() {
        let temp = tempfile::NamedTempFile::new().unwrap();

        let tool = WriteFileTool;
        let args = serde_json::json!({
            "path": temp.path().to_str().unwrap(),
            "content": "hello "
        });
        tool.run(args, CancellationToken::new()).await.unwrap();

        let args = serde_json::json!({
            "path": temp.path().to_str().unwrap(),
            "content": "world",
            "append": true
        });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert!(result.contains("appended"));
        let read = tokio::fs::read_to_string(temp.path()).await.unwrap();
        assert_eq!(read, "hello world");
    }
}
