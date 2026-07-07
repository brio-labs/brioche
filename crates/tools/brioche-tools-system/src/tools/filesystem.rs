//! File system tools.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_shell_runtime::{ToolSchemaProperty, ToolSchemaPropertyType, tool_parameters_schema};
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

/// Sandbox configuration for filesystem tools.
///
/// Refs: docs/SPECS.md §Book III-C
#[derive(Clone, Debug, Default)]
pub struct FileSystemSandbox {
    base_dir: Option<std::path::PathBuf>,
    allow_absolute: bool,
}

impl FileSystemSandbox {
    /// Creates a sandbox rooted at the current working directory,
    /// rejecting absolute paths by default.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the base directory for relative paths.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_base_dir(mut self, base_dir: impl Into<std::path::PathBuf>) -> Self {
        self.base_dir = Some(base_dir.into());
        self
    }

    /// Allows absolute paths anywhere.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn allow_absolute(mut self) -> Self {
        self.allow_absolute = true;
        self
    }
}

/// Resolves a user-supplied path against the sandbox.
///
/// Refs: docs/SPECS.md §Book III-C
fn resolve_path(raw: &str, sandbox: &FileSystemSandbox) -> Result<std::path::PathBuf, ToolError> {
    let expanded = expand_tilde(raw);
    let path = std::path::PathBuf::from(expanded);

    if path.is_absolute() {
        if sandbox.allow_absolute {
            return Ok(path);
        }
        if let Some(base) = &sandbox.base_dir {
            let base_abs = std::path::absolute(base)?;
            let path_norm = normalize_path(&path);
            let base_norm = normalize_path(&base_abs);
            if path_norm.starts_with(&base_norm) {
                return Ok(path);
            }
        }
        return Err(ToolError::SandboxDenied(format!(
            "absolute path '{}' is not allowed",
            raw
        )));
    }

    let base_result = match sandbox.base_dir.as_deref() {
        Some(base) => std::path::absolute(base),
        None => std::env::current_dir(),
    };
    let base = base_result.map_err(ToolError::Io)?;
    let resolved = base.join(&path);
    let resolved_norm = normalize_path(&resolved);
    let base_norm = normalize_path(&base);
    if !resolved_norm.starts_with(&base_norm) {
        return Err(ToolError::SandboxDenied(format!(
            "path '{}' escapes base directory '{}'",
            raw,
            base.display()
        )));
    }
    Ok(resolved_norm)
}

/// Lexically normalizes a path without touching the filesystem.
///
/// Refs: docs/SPECS.md §Book III-C
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(_) => {
                normalized.push(component.as_os_str());
            }
            std::path::Component::RootDir => {
                normalized.push(std::path::MAIN_SEPARATOR_STR);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            std::path::Component::Normal(c) => {
                normalized.push(c);
            }
        }
    }
    normalized
}

/// Reads the contents of a text file.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Default)]
pub struct ReadFileTool {
    sandbox: FileSystemSandbox,
}

impl ReadFileTool {
    /// Creates a new `ReadFileTool` with a base directory for resolving relative paths.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new(base_dir: Option<std::path::PathBuf>) -> Self {
        let mut sandbox = FileSystemSandbox::new();
        if let Some(base_dir) = base_dir {
            sandbox = sandbox.with_base_dir(base_dir);
        }
        Self { sandbox }
    }

    /// Allows absolute paths anywhere.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_allow_absolute(mut self, allow: bool) -> Self {
        self.sandbox = self.sandbox.allow_absolute();
        if !allow {
            self.sandbox.allow_absolute = false;
        }
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
        tool_parameters_schema(&[ToolSchemaProperty::new(
            "path",
            ToolSchemaPropertyType::String,
            "Absolute or relative path to the file",
            true,
        )])
    }

    async fn run(
        &self,
        args: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let path_raw = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let path = resolve_path(path_raw, &self.sandbox)?;
        let content = tokio::fs::read_to_string(path).await?;
        Ok(content)
    }
}

/// Writes content to a text file.
/// Refs: docs/SPECS.md §Book III-C
#[derive(Default)]
pub struct WriteFileTool {
    sandbox: FileSystemSandbox,
}

impl WriteFileTool {
    /// Creates a new `WriteFileTool` with a base directory for resolving relative paths.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new(base_dir: Option<std::path::PathBuf>) -> Self {
        let mut sandbox = FileSystemSandbox::new();
        if let Some(base_dir) = base_dir {
            sandbox = sandbox.with_base_dir(base_dir);
        }
        Self { sandbox }
    }

    /// Allows absolute paths anywhere.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_allow_absolute(mut self, allow: bool) -> Self {
        self.sandbox = self.sandbox.allow_absolute();
        if !allow {
            self.sandbox.allow_absolute = false;
        }
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
        tool_parameters_schema(&[
            ToolSchemaProperty::new(
                "path",
                ToolSchemaPropertyType::String,
                "Absolute or relative path to the file",
                true,
            ),
            ToolSchemaProperty::new(
                "content",
                ToolSchemaPropertyType::String,
                "Text content to write",
                true,
            ),
            ToolSchemaProperty::new(
                "append",
                ToolSchemaPropertyType::Boolean,
                "If true, append to the file instead of overwriting",
                false,
            ),
        ])
    }

    async fn run(
        &self,
        args: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let path_raw = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let path = resolve_path(path_raw, &self.sandbox)?;
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
    sandbox: FileSystemSandbox,
}

impl ListDirTool {
    /// Creates a new `ListDirTool` with a base directory for resolving relative paths.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn new(base_dir: Option<std::path::PathBuf>) -> Self {
        let mut sandbox = FileSystemSandbox::new();
        if let Some(base_dir) = base_dir {
            sandbox = sandbox.with_base_dir(base_dir);
        }
        Self { sandbox }
    }

    /// Allows absolute paths anywhere.
    ///
    /// Refs: docs/SPECS.md §Book III-C
    pub fn with_allow_absolute(mut self, allow: bool) -> Self {
        self.sandbox = self.sandbox.allow_absolute();
        if !allow {
            self.sandbox.allow_absolute = false;
        }
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
        tool_parameters_schema(&[ToolSchemaProperty::new(
            "path",
            ToolSchemaPropertyType::String,
            "Absolute or relative path to the directory",
            true,
        )])
    }

    async fn run(
        &self,
        args: serde_json::Value,
        _cancel: CancellationToken,
    ) -> Result<String, ToolError> {
        let path_raw = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgs("missing 'path'".into()))?;
        let path = resolve_path(path_raw, &self.sandbox)?;
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

    #[test]
    fn read_file_schema_preserves_owned_shape() {
        let tool = ReadFileTool::default();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(
            schema["properties"]["path"]["description"],
            "Absolute or relative path to the file"
        );
        assert_eq!(schema["required"], serde_json::json!(["path"]));
    }

    #[tokio::test]
    async fn write_file_appends_content() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("file.txt");

        let tool = WriteFileTool::new(Some(temp.path().to_path_buf()));
        let args = serde_json::json!({
            "path": "file.txt",
            "content": "hello "
        });
        tool.run(args, CancellationToken::new()).await.unwrap();

        let args = serde_json::json!({
            "path": "file.txt",
            "content": "world",
            "append": true
        });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();

        assert!(result.contains("appended"));
        let read = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(read, "hello world");
    }

    #[tokio::test]
    async fn read_file_rejects_absolute_ssh_key_by_default() {
        let tool = ReadFileTool::default();
        let args = serde_json::json!({ "path": "~/.ssh/id_rsa" });
        let result = tool.run(args, CancellationToken::new()).await;
        assert!(
            matches!(result, Err(ToolError::SandboxDenied(_))),
            "expected sandbox denied, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn read_file_allows_relative_path_inside_base_dir() {
        let temp = tempfile::TempDir::new().unwrap();
        let file = temp.path().join("file.txt");
        tokio::fs::write(&file, "hello").await.unwrap();

        let tool = ReadFileTool::new(Some(temp.path().to_path_buf()));
        let args = serde_json::json!({ "path": "file.txt" });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn read_file_rejects_relative_escape() {
        let temp = tempfile::TempDir::new().unwrap();
        let tool = ReadFileTool::new(Some(temp.path().to_path_buf()));
        let args = serde_json::json!({ "path": "../escape.txt" });
        let result = tool.run(args, CancellationToken::new()).await;
        assert!(
            matches!(result, Err(ToolError::SandboxDenied(_))),
            "expected sandbox denied, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn write_file_rejects_absolute_path_by_default() {
        let tool = WriteFileTool::default();
        let args = serde_json::json!({
            "path": "/tmp/should-not-write.txt",
            "content": "secret"
        });
        let result = tool.run(args, CancellationToken::new()).await;
        assert!(
            matches!(result, Err(ToolError::SandboxDenied(_))),
            "expected sandbox denied, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn write_file_allows_absolute_path_when_opted_in() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let tool = WriteFileTool::default().with_allow_absolute(true);
        let args = serde_json::json!({
            "path": temp.path().to_str().unwrap(),
            "content": "hello"
        });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();
        assert!(result.contains("written"));
        let read = tokio::fs::read_to_string(temp.path()).await.unwrap();
        assert_eq!(read, "hello");
    }

    #[tokio::test]
    async fn list_dir_rejects_absolute_path_outside_base() {
        let tool = ListDirTool::default();
        let args = serde_json::json!({ "path": "/etc" });
        let result = tool.run(args, CancellationToken::new()).await;
        assert!(
            matches!(result, Err(ToolError::SandboxDenied(_))),
            "expected sandbox denied, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn list_dir_lists_base_dir_contents() {
        let temp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(temp.path().join("file.txt"), "hello")
            .await
            .unwrap();
        tokio::fs::create_dir(temp.path().join("dir"))
            .await
            .unwrap();

        let tool = ListDirTool::new(Some(temp.path().to_path_buf()));
        let args = serde_json::json!({ "path": "." });
        let result = tool.run(args, CancellationToken::new()).await.unwrap();
        assert!(result.contains("file file.txt"));
        assert!(result.contains("dir dir"));
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
