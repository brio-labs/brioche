//! File system tools.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::registry::{SystemTool, ToolError};

/// Resolve a user-supplied path against the tool's sandbox.
///
/// By default the sandbox rejects absolute paths, leading `~`, and any
/// path that resolves outside `base_dir`. Setting `allow_absolute` disables
/// all sandbox checks and permits any path.
///
/// Refs: I-Shell-Runtime-OnlyIO
fn resolve_sandboxed_path(
    raw: &str,
    base_dir: &Path,
    allow_absolute: bool,
) -> Result<PathBuf, ToolError> {
    if allow_absolute {
        let expanded = expand_tilde(raw);
        return Ok(PathBuf::from(expanded));
    }

    if raw.starts_with('~') {
        return Err(ToolError::SandboxDenied(
            "tilde paths are not allowed".into(),
        ));
    }

    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(ToolError::SandboxDenied(
            "absolute paths are not allowed".into(),
        ));
    }

    let resolved = normalize_path(&base_dir.join(path));
    let canonical_base = normalize_path(base_dir);

    if !resolved.starts_with(&canonical_base) {
        return Err(ToolError::SandboxDenied(format!(
            "path '{}' resolves outside the sandbox",
            raw
        )));
    }

    Ok(resolved)
}

/// Normalize a path by collapsing `.` and `..` without touching the filesystem.
///
/// This is a pure string normalization used for sandbox containment checks.
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => {
                result.push(prefix.as_os_str());
            }
            std::path::Component::RootDir => {
                result.push(std::path::MAIN_SEPARATOR_STR);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !result.pop() {
                    // Leading `..` would escape above the root; keep it as an
                    // explicit marker so `starts_with` later fails containment.
                    result.push("..");
                }
            }
            std::path::Component::Normal(name) => {
                result.push(name);
            }
        }
    }
    result
}

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
    base_dir: PathBuf,
    allow_absolute: bool,
}

impl ReadFileTool {
    /// Creates a new `ReadFileTool` with a base directory for resolving relative paths.
    ///
    /// `None` defaults to the current working directory.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            base_dir: match base_dir {
                Some(dir) => dir,
                None => match std::env::current_dir() {
                    Ok(dir) => dir,
                    Err(_) => PathBuf::from("."),
                },
            },
            allow_absolute: false,
        }
    }

    /// Allows absolute paths and disables the base-directory sandbox.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_allow_absolute(mut self, allow: bool) -> Self {
        self.allow_absolute = allow;
        self
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
            &[("path", "Relative path to the file within the workspace")],
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
        let path = resolve_sandboxed_path(path_raw, &self.base_dir, self.allow_absolute)?;
        let content = tokio::fs::read_to_string(path).await?;
        Ok(content)
    }
}

/// Writes content to a text file.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Default)]
pub struct WriteFileTool {
    base_dir: PathBuf,
    allow_absolute: bool,
}

impl WriteFileTool {
    /// Creates a new `WriteFileTool` with a base directory for resolving relative paths.
    ///
    /// `None` defaults to the current working directory.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            base_dir: match base_dir {
                Some(dir) => dir,
                None => match std::env::current_dir() {
                    Ok(dir) => dir,
                    Err(_) => PathBuf::from("."),
                },
            },
            allow_absolute: false,
        }
    }

    /// Allows absolute paths and disables the base-directory sandbox.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_allow_absolute(mut self, allow: bool) -> Self {
        self.allow_absolute = allow;
        self
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
            "Relative path to the file within the workspace".into(),
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
        let path = resolve_sandboxed_path(path_raw, &self.base_dir, self.allow_absolute)?;
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
#[derive(Default)]
pub struct ListDirTool {
    base_dir: PathBuf,
    allow_absolute: bool,
}

impl ListDirTool {
    /// Creates a new `ListDirTool` with a base directory for resolving relative paths.
    ///
    /// `None` defaults to the current working directory.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        Self {
            base_dir: match base_dir {
                Some(dir) => dir,
                None => match std::env::current_dir() {
                    Ok(dir) => dir,
                    Err(_) => PathBuf::from("."),
                },
            },
            allow_absolute: false,
        }
    }

    /// Allows absolute paths and disables the base-directory sandbox.
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_allow_absolute(mut self, allow: bool) -> Self {
        self.allow_absolute = allow;
        self
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
            &[(
                "path",
                "Relative path to the directory within the workspace",
            )],
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
        let path = resolve_sandboxed_path(path_raw, &self.base_dir, self.allow_absolute)?;
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

        let tool = WriteFileTool::new(Some(temp.path().parent().unwrap().to_path_buf()));
        let file_name = temp.path().file_name().unwrap().to_str().unwrap();
        let args = serde_json::json!({
            "path": file_name,
            "content": "hello "
        });
        tool.run(args, CancellationToken::new()).await.unwrap();

        let args = serde_json::json!({
            "path": file_name,
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

    #[tokio::test]
    async fn read_file_rejects_absolute_path_by_default() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let tool = ReadFileTool::new(Some(temp.path().parent().unwrap().to_path_buf()));
        let args = serde_json::json!({
            "path": temp.path().to_str().unwrap(),
        });
        let err = tool.run(args, CancellationToken::new()).await.unwrap_err();
        assert!(
            matches!(err, ToolError::SandboxDenied(_)),
            "expected sandbox denial, got {err:?}"
        );
    }

    #[tokio::test]
    async fn read_file_rejects_tilde_path_by_default() {
        let tool = ReadFileTool::new(Some(PathBuf::from("/tmp")));
        let args = serde_json::json!({
            "path": "~/.ssh/id_rsa",
        });
        let err = tool.run(args, CancellationToken::new()).await.unwrap_err();
        assert!(
            matches!(err, ToolError::SandboxDenied(_)),
            "expected sandbox denial, got {err:?}"
        );
    }

    #[tokio::test]
    async fn read_file_rejects_escape_attempt() {
        let base = tempfile::tempdir().unwrap();
        let tool = ReadFileTool::new(Some(base.path().to_path_buf()));
        let args = serde_json::json!({
            "path": "../etc/passwd",
        });
        let err = tool.run(args, CancellationToken::new()).await.unwrap_err();
        assert!(
            matches!(err, ToolError::SandboxDenied(_)),
            "expected sandbox denial, got {err:?}"
        );
    }

    #[tokio::test]
    async fn read_file_allows_absolute_path_when_overridden() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        tokio::fs::write(temp.path(), "secret").await.unwrap();
        let tool = ReadFileTool::new(Some(PathBuf::from("/tmp"))).with_allow_absolute(true);
        let args = serde_json::json!({
            "path": temp.path().to_str().unwrap(),
        });
        let content = tool.run(args, CancellationToken::new()).await.unwrap();
        assert_eq!(content, "secret");
    }

    #[tokio::test]
    async fn write_file_creates_file_within_base_dir() {
        let base = tempfile::tempdir().unwrap();
        let tool = WriteFileTool::new(Some(base.path().to_path_buf()));
        let args = serde_json::json!({
            "path": "subdir/file.txt",
            "content": "hello"
        });
        tool.run(args, CancellationToken::new()).await.unwrap();
        let content = tokio::fs::read_to_string(base.path().join("subdir/file.txt"))
            .await
            .unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn list_dir_lists_relative_directory() {
        let base = tempfile::tempdir().unwrap();
        tokio::fs::create_dir(base.path().join("inner"))
            .await
            .unwrap();
        let tool = ListDirTool::new(Some(base.path().to_path_buf()));
        let args = serde_json::json!({
            "path": "inner",
        });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();
        assert!(result.is_empty());
    }
}
