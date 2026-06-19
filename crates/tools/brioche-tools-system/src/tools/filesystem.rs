//! File system tools.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::registry::{SystemTool, ToolError};

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
/// Refs: docs/SPECS.md §Book III-C
#[derive(Default)]
pub struct ReadFileTool {
    base_dir: Option<std::path::PathBuf>,
}

impl ReadFileTool {
    /// Creates a new `ReadFileTool` with a base directory for resolving relative paths.
    pub fn new(base_dir: Option<std::path::PathBuf>) -> Self {
        Self { base_dir }
    }
}

#[async_trait::async_trait]
impl SystemTool for ReadFileTool {
    fn name(&self) -> String {
        "read_file".into()
    }

    fn description(&self) -> String {
        "Read the contents of a text file.".into()
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
        let path_raw = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let path_expanded = expand_tilde(path_raw);
        let path = if !std::path::Path::new(&path_expanded).is_absolute() {
            if let Some(ref base) = self.base_dir {
                base.join(&path_expanded)
            } else {
                std::path::PathBuf::from(path_expanded)
            }
        } else {
            std::path::PathBuf::from(path_expanded)
        };
        let content = tokio::fs::read_to_string(path).await?;
        Ok(content)
    }
}

/// Writes content to a text file.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Default)]
pub struct WriteFileTool {
    base_dir: Option<std::path::PathBuf>,
}

impl WriteFileTool {
    /// Creates a new `WriteFileTool` with a base directory for resolving relative paths.
    pub fn new(base_dir: Option<std::path::PathBuf>) -> Self {
        Self { base_dir }
    }
}

#[async_trait::async_trait]
impl SystemTool for WriteFileTool {
    fn name(&self) -> String {
        "write_file".into()
    }

    fn description(&self) -> String {
        "Write content to a text file. Creates the file if it does not exist.".into()
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
        _cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let path_raw = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let path_expanded = expand_tilde(path_raw);
        let path = if !std::path::Path::new(&path_expanded).is_absolute() {
            if let Some(ref base) = self.base_dir {
                base.join(&path_expanded)
            } else {
                std::path::PathBuf::from(path_expanded)
            }
        } else {
            std::path::PathBuf::from(path_expanded)
        };
        let content = args["content"].as_str().map_or("", |v| v);
        let append = args["append"].as_bool().is_some_and(|v| v);

        // Create parent directories if they don't exist.
        if let Some(parent) = path.parent() {
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
            Ok(format!(
                "appended {} bytes to {}",
                content.len(),
                path.display()
            ))
        } else {
            tokio::fs::write(&path, content).await?;
            Ok(format!(
                "written {} bytes to {}",
                content.len(),
                path.display()
            ))
        }
    }
}

/// Lists the contents of a directory.
/// Refs: docs/SPECS.md §Book III-C
/// Lists the contents of a directory.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Default)]
pub struct ListDirTool {
    base_dir: Option<std::path::PathBuf>,
}

impl ListDirTool {
    /// Creates a new `ListDirTool` with a base directory for resolving relative paths.
    pub fn new(base_dir: Option<std::path::PathBuf>) -> Self {
        Self { base_dir }
    }
}

#[async_trait::async_trait]
impl SystemTool for ListDirTool {
    fn name(&self) -> String {
        "list_dir".into()
    }

    fn description(&self) -> String {
        "List the contents of a directory.".into()
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
        let path_raw = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let path_expanded = expand_tilde(path_raw);
        let path = if !std::path::Path::new(&path_expanded).is_absolute() {
            if let Some(ref base) = self.base_dir {
                base.join(&path_expanded)
            } else {
                std::path::PathBuf::from(path_expanded)
            }
        } else {
            std::path::PathBuf::from(path_expanded)
        };
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
#[allow(clippy::disallowed_methods)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn write_file_schema_includes_append() {
        let tool = WriteFileTool::default();
        let schema = tool.parameters_schema();
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

        let tool = WriteFileTool::default();
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

    #[test]
    fn expand_tilde_with_home() {
        if std::env::var("HOME").is_ok() {
            let expanded = expand_tilde("~/test.txt");
            assert!(!expanded.starts_with('~'));
        }
    }

    #[test]
    fn expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/tmp/test.txt"), "/tmp/test.txt");
    }
}
