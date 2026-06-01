//! Builds the OpenAI JSON request from Brioche chat history.
//!
//! Converts `ChatMessage` variants into the JSON format expected by
//! the OpenAI Chat Completions API, and injects available tools via
//! their JSON Schema.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_shell_runtime::ChatMessage;

/// Builds a simple `{role, content}` message object.
fn simple_msg(role: &str, content: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert("role".into(), serde_json::Value::String(role.into()));
    m.insert("content".into(), serde_json::Value::String(content.into()));
    serde_json::Value::Object(m)
}

/// Converts Brioche history into an array of OpenAI messages.
///
/// `ToolRequest` and `ToolResult` variants are mapped to the
/// `tool_calls` / `tool` format expected by OpenAI.
///
/// Consecutive `ToolRequest` messages are grouped into a single
/// assistant message with multiple `tool_calls`.
///
/// # Complexity
/// O(n) where n = number of messages. One `Vec` allocation.
pub fn build_messages(history: &[ChatMessage]) -> Vec<serde_json::Value> {
    let mut result = Vec::with_capacity(history.len());
    let mut i = 0;

    while i < history.len() {
        match &history[i] {
            ChatMessage::System { content } => {
                result.push(simple_msg("system", content));
                i += 1;
            }
            ChatMessage::User { content } => {
                result.push(simple_msg("user", content));
                i += 1;
            }
            ChatMessage::Assistant { content } => {
                result.push(simple_msg("assistant", content));
                i += 1;
            }
            ChatMessage::ToolRequest { .. } => {
                // Group consecutive ToolRequest into a single assistant message.
                let mut tool_calls = Vec::new();
                while i < history.len() {
                    if let ChatMessage::ToolRequest {
                        id,
                        name,
                        arguments,
                    } = &history[i]
                    {
                        let mut func = serde_json::Map::new();
                        func.insert("name".into(), serde_json::Value::String(name.clone()));
                        func.insert(
                            "arguments".into(),
                            serde_json::Value::String(arguments.clone()),
                        );

                        let mut tool_call = serde_json::Map::new();
                        tool_call.insert("id".into(), serde_json::Value::String(id.clone()));
                        tool_call
                            .insert("type".into(), serde_json::Value::String("function".into()));
                        tool_call.insert("function".into(), serde_json::Value::Object(func));
                        tool_calls.push(serde_json::Value::Object(tool_call));
                        i += 1;
                    } else {
                        break;
                    }
                }

                let mut m = serde_json::Map::new();
                m.insert("role".into(), serde_json::Value::String("assistant".into()));
                m.insert("content".into(), serde_json::Value::Null);
                m.insert("tool_calls".into(), serde_json::Value::Array(tool_calls));
                result.push(serde_json::Value::Object(m));
            }
            ChatMessage::ToolResult { id, content } => {
                let mut m = serde_json::Map::new();
                m.insert("role".into(), serde_json::Value::String("tool".into()));
                m.insert("tool_call_id".into(), serde_json::Value::String(id.clone()));
                m.insert("content".into(), serde_json::Value::String(content.clone()));
                result.push(serde_json::Value::Object(m));
                i += 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_messages_groups_consecutive_tool_requests() {
        let history = vec![
            ChatMessage::User {
                content: "read and write".into(),
            },
            ChatMessage::ToolRequest {
                id: "call_1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"/tmp/a\"}".into(),
            },
            ChatMessage::ToolRequest {
                id: "call_2".into(),
                name: "write_file".into(),
                arguments: "{\"path\":\"/tmp/b\",\"content\":\"hi\"}".into(),
            },
            ChatMessage::ToolResult {
                id: "call_1".into(),
                content: "hello".into(),
            },
            ChatMessage::ToolResult {
                id: "call_2".into(),
                content: "done".into(),
            },
            ChatMessage::Assistant {
                content: "All done".into(),
            },
        ];

        let messages = build_messages(&history);
        assert_eq!(messages.len(), 5);

        // User
        assert_eq!(messages[0]["role"], "user");

        // Assistant with 2 tool_calls
        assert_eq!(messages[1]["role"], "assistant");
        assert!(messages[1]["content"].is_null());
        let tool_calls = messages[1]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(tool_calls[0]["function"]["name"], "read_file");
        assert_eq!(tool_calls[1]["id"], "call_2");
        assert_eq!(tool_calls[1]["function"]["name"], "write_file");

        // Tool results
        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_1");
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "call_2");

        // Final assistant
        assert_eq!(messages[4]["role"], "assistant");
        assert_eq!(messages[4]["content"], "All done");
    }

    #[test]
    fn build_messages_single_tool_request() {
        let history = vec![
            ChatMessage::User {
                content: "hello".into(),
            },
            ChatMessage::ToolRequest {
                id: "call_1".into(),
                name: "read_file".into(),
                arguments: "{\"path\":\"/tmp/test\"}".into(),
            },
        ];

        let messages = build_messages(&history);
        assert_eq!(messages.len(), 2);
        let tool_calls = messages[1]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(
            tool_calls[0]["function"]["arguments"],
            "{\"path\":\"/tmp/test\"}"
        );
    }
}

/// Assembles the Chat Completions request body.
///
/// `tools` is optional. When provided, the `tools` field is injected
/// into the payload to enable function calling mode.
pub fn build_request_body(
    model: &str,
    messages: Vec<serde_json::Value>,
    max_tokens: u32,
    tools: Option<&[serde_json::Value]>,
) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), serde_json::Value::String(model.into()));
    body.insert("messages".into(), serde_json::Value::Array(messages));
    body.insert(
        "max_tokens".into(),
        serde_json::Value::Number(max_tokens.into()),
    );
    body.insert("stream".into(), serde_json::Value::Bool(true));

    if let Some(tools) = tools
        && !tools.is_empty()
    {
        body.insert("tools".into(), serde_json::Value::Array(tools.to_vec()));
        body.insert(
            "tool_choice".into(),
            serde_json::Value::String("auto".into()),
        );
    }

    serde_json::Value::Object(body)
}
