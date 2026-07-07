//! Tests for OpenAI request builder.
//!
//! Covers `build_messages` and `build_request_body`.

#![allow(clippy::disallowed_methods, clippy::unwrap_used, clippy::panic)]
use brioche_core::ChatMessage;
use brioche_provider_openai::request::{build_messages, build_request_body};

fn simple_msg(role: &str, content: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("role".into(), serde_json::Value::String(role.into()));
    m.insert("content".into(), serde_json::Value::String(content.into()));
    serde_json::Value::Object(m)
}

#[test]
fn build_messages_empty_history() {
    let history: Vec<ChatMessage> = vec![];
    let messages = build_messages(&history);
    assert!(messages.is_empty());
}

#[test]
fn build_messages_single_user_message() {
    let history = vec![ChatMessage::User {
        content: "Hello".to_string(),
    }];
    let messages = build_messages(&history);
    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg["role"], "user");
    assert_eq!(msg["content"], "Hello");
}

#[test]
fn build_messages_system_and_user() {
    let history = vec![
        ChatMessage::System {
            content: "You are a helper".to_string(),
        },
        ChatMessage::User {
            content: "Hello".to_string(),
        },
    ];
    let messages = build_messages(&history);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
}

#[test]
fn build_messages_truncates_tool_result() {
    let long_content = "x".repeat(5000);
    let history = vec![ChatMessage::ToolResult {
        id: "call-1".to_string(),
        content: long_content,
    }];
    let messages = build_messages(&history);
    assert_eq!(messages.len(), 1);
    let content = messages[0]["content"].as_str();
    assert!(content.is_some());
    let content = content.unwrap();
    assert!(content.len() <= 4000);
    assert!(content.ends_with("..."));
}

#[test]
fn build_request_body_includes_model_and_stream() {
    let messages = vec![simple_msg("user", "hi")];
    let body = build_request_body("gpt-4o", messages, 4096, None, None, true);
    assert_eq!(body["model"], "gpt-4o");
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_tokens"], 4096);
    assert!(body.get("tools").is_none());
}

#[test]
fn build_request_body_includes_tools_when_provided() {
    let messages = vec![simple_msg("user", "hi")];
    let mut tool_func = serde_json::Map::new();
    tool_func.insert("name".into(), serde_json::Value::String("test".into()));
    let mut tool = serde_json::Map::new();
    tool.insert("type".into(), serde_json::Value::String("function".into()));
    tool.insert("function".into(), serde_json::Value::Object(tool_func));
    let tools = vec![serde_json::Value::Object(tool)];
    let body = build_request_body("gpt-4o", messages, 4096, None, Some(tools), true);
    assert!(body.get("tools").is_some());
    assert_eq!(body["tool_choice"], "auto");
}

#[test]
fn build_request_body_includes_reasoning_effort() {
    let messages = vec![simple_msg("user", "hi")];
    let body = build_request_body("gpt-4o", messages, 4096, Some("high"), None, true);
    assert_eq!(body["reasoning"]["effort"], "high");
}

#[test]
fn build_messages_limits_to_max_messages() {
    let mut history = vec![ChatMessage::System {
        content: "system prompt".to_string(),
    }];
    for i in 0..35 {
        history.push(ChatMessage::User {
            content: format!("message {}", i),
        });
    }
    let messages = build_messages(&history);
    // Should keep system + 29 most recent = 30 total
    assert_eq!(messages.len(), 30);
    assert_eq!(messages[0]["role"], "system");
}
