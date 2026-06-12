//! Integration tests for `brioche-tools-system`.
//!
//! Covers idempotency of filesystem writes and sandboxing of shell commands.
//!
//! Refs: SPECS.md §Book III-C, I-Shell-Runtime-OnlyIO

#![allow(clippy::disallowed_methods, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use brioche_core::ActiveToolCall;
use brioche_shell_runtime::ToolExecutor;
use brioche_tools_system::{
    AllowList, ExecuteCommandTool, SandboxPolicy, SystemTool, SystemToolExecutor, WriteFileTool,
};
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn write_file_is_idempotent() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_str().unwrap();

    let tool = WriteFileTool;
    let args = serde_json::json!({ "path": path, "content": "hello" });
    let first = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert!(first.contains("written"), "first result: {first}");

    let second = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert!(second.contains("written"), "second result: {second}");

    let content = tokio::fs::read_to_string(path).await.unwrap();
    assert_eq!(content, "hello");
}

#[tokio::test]
async fn write_file_append_is_idempotent() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    let path = temp.path().to_str().unwrap();

    let tool = WriteFileTool;
    let args = serde_json::json!({ "path": path, "content": "a", "append": true });
    let first = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert!(first.contains("appended"));

    let second = tool
        .run(args.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert!(second.contains("appended"));

    let content = tokio::fs::read_to_string(path).await.unwrap();
    assert_eq!(content, "aa");
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
        other => panic!("expected BusinessError, got {other:?}"),
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

    let args = serde_json::json!({ "command": "echo ok" });
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

    let args = serde_json::json!({ "command": "echo ok" });
    let result = tool.run(args, CancellationToken::new()).await;

    assert!(result.is_ok(), "expected success, got {result:?}");
}
