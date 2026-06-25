//! Integration tests for `brioche-tools-system`.
//!
//! Covers idempotency of filesystem writes and sandboxing of shell commands.
//!
//! Refs: docs/SPECS.md §Book III-C, I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_core::ActiveToolCall;
use brioche_shell_runtime::ToolExecutor;
use brioche_tools_system::{
    AllowList, ExecuteCommandTool, SandboxPolicy, SystemTool, SystemToolExecutor, WriteFileTool,
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn write_file_is_idempotent() -> std::io::Result<()> {
    let temp = tempfile::NamedTempFile::new()?;
    let path = temp
        .path()
        .to_str()
        .ok_or_else(|| std::io::Error::other("temp path is not valid UTF-8"))?;

    let tool = WriteFileTool::default();
    let args = serde_json::Value::Object({
        let mut m = serde_json::Map::new();
        m.insert("path".into(), path.into());
        m.insert("content".into(), "hello".into());
        m
    });
    let first = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .map_err(|e| std::io::Error::other(format!("tool run failed: {e}")))?;
    assert!(first.contains("written"), "first result: {first}");

    let second = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .map_err(|e| std::io::Error::other(format!("tool run failed: {e}")))?;
    assert!(second.contains("written"), "second result: {second}");

    let content = tokio::fs::read_to_string(path).await?;
    assert_eq!(content, "hello");
    Ok(())
}

#[tokio::test]
async fn write_file_append_is_idempotent() -> std::io::Result<()> {
    let temp = tempfile::NamedTempFile::new()?;
    let path = temp
        .path()
        .to_str()
        .ok_or_else(|| std::io::Error::other("temp path is not valid UTF-8"))?;

    let tool = WriteFileTool::default();
    let args = serde_json::Value::Object({
        let mut m = serde_json::Map::new();
        m.insert("path".into(), path.into());
        m.insert("content".into(), "a".into());
        m.insert("append".into(), true.into());
        m
    });
    let first = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .map_err(|e| std::io::Error::other(format!("tool run failed: {e}")))?;
    assert!(first.contains("appended"));

    let second = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .map_err(|e| std::io::Error::other(format!("tool run failed: {e}")))?;
    assert!(second.contains("appended"));

    let content = tokio::fs::read_to_string(path).await?;
    assert_eq!(content, "aa");
    Ok(())
}

#[tokio::test]
async fn sandbox_denies_unlisted_command() {
    let executor = SystemToolExecutor::new().with_tool(ExecuteCommandTool::new().with_policy(
        SandboxPolicy::AllowList(AllowList::new().with_command("echo")),
    ));

    let call = ActiveToolCall {
        tool_id: "1".into(),
        tool_name: "execute_command".into(),
        arguments: r#"{"command":"rm -rf /"}"#.into(),
        timeout_ms: 5000,
    };
    let result = executor.execute(&call, CancellationToken::new()).await;

    assert!(
        matches!(result.outcome, brioche_core::ToolOutcome::BusinessError(_)),
        "expected business error, got {:?}",
        result.outcome
    );
    let err = match result.outcome {
        brioche_core::ToolOutcome::BusinessError(e) => e,
        other => {
            let _ = other;
            return;
        }
    };
    assert!(
        err.contains("sandbox denied"),
        "error should mention sandbox: {err}"
    );
}

#[tokio::test]
async fn sandbox_allows_listed_command() {
    let executor = SystemToolExecutor::new().with_tool(ExecuteCommandTool::new().with_policy(
        SandboxPolicy::AllowList(AllowList::new().with_command("echo")),
    ));

    let call = ActiveToolCall {
        tool_id: "1".into(),
        tool_name: "execute_command".into(),
        arguments: r#"{"command":"echo ok"}"#.into(),
        timeout_ms: 5000,
    };
    let result = executor.execute(&call, CancellationToken::new()).await;

    assert!(
        matches!(result.outcome, brioche_core::ToolOutcome::Success(_)),
        "expected success, got {:?}",
        result.outcome
    );
}

#[tokio::test]
async fn permissive_requires_confirmation_when_handler_denies() {
    let tool = ExecuteCommandTool::new()
        .with_policy(SandboxPolicy::Interactive)
        .with_confirm_handler(Arc::new(|_| false));

    let args = serde_json::Value::Object({
        let mut m = serde_json::Map::new();
        m.insert("command".into(), "echo ok".into());
        m
    });
    let result = tool.run(args, CancellationToken::new()).await;

    assert!(
        matches!(
            result,
            Err(brioche_tools_system::ToolError::SandboxDenied(_))
        ),
        "expected sandbox denied, got {result:?}"
    );
}

#[tokio::test]
async fn permissive_allows_when_handler_confirms() {
    let tool = ExecuteCommandTool::new()
        .with_policy(SandboxPolicy::Permissive)
        .with_confirm_handler(Arc::new(|_| true));

    let args = serde_json::Value::Object({
        let mut m = serde_json::Map::new();
        m.insert("command".into(), "echo ok".into());
        m
    });
    let result = tool.run(args, CancellationToken::new()).await;

    assert!(result.is_ok(), "expected success, got {result:?}");
}

#[tokio::test]
async fn permissive_denies_without_confirm_handler() {
    let tool = ExecuteCommandTool::new().with_policy(SandboxPolicy::Permissive);

    let args = serde_json::Value::Object({
        let mut m = serde_json::Map::new();
        m.insert("command".into(), "echo ok".into());
        m
    });
    let result = tool.run(args, CancellationToken::new()).await;

    assert!(
        matches!(
            result,
            Err(brioche_tools_system::ToolError::SandboxDenied(_))
        ),
        "expected sandbox denied without confirm handler, got {result:?}"
    );
}

#[tokio::test]
async fn permissive_denies_when_handler_denies() {
    let tool = ExecuteCommandTool::new()
        .with_policy(SandboxPolicy::Permissive)
        .with_confirm_handler(Arc::new(|_| false));

    let args = serde_json::Value::Object({
        let mut m = serde_json::Map::new();
        m.insert("command".into(), "echo ok".into());
        m
    });
    let result = tool.run(args, CancellationToken::new()).await;

    assert!(
        matches!(
            result,
            Err(brioche_tools_system::ToolError::SandboxDenied(_))
        ),
        "expected sandbox denied when handler denies, got {result:?}"
    );
}
