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
/// # Complexity
/// O(n) where n = number of messages. One `Vec` allocation.
pub fn build_messages(history: &[ChatMessage]) -> Vec<serde_json::Value> {
    history
        .iter()
        .map(|message| match message {
            ChatMessage::System { content } => simple_msg("system", content),
            ChatMessage::User { content } => simple_msg("user", content),
            ChatMessage::Assistant { content } => simple_msg("assistant", content),
            ChatMessage::ToolRequest {
                id,
                name,
                arguments,
            } => {
                let mut func = serde_json::Map::new();
                func.insert("name".into(), serde_json::Value::String(name.clone()));
                func.insert(
                    "arguments".into(),
                    serde_json::Value::String(arguments.clone()),
                );

                let mut tool_call = serde_json::Map::new();
                tool_call.insert("id".into(), serde_json::Value::String(id.clone()));
                tool_call.insert("type".into(), serde_json::Value::String("function".into()));
                tool_call.insert("function".into(), serde_json::Value::Object(func));

                let mut m = serde_json::Map::new();
                m.insert("role".into(), serde_json::Value::String("assistant".into()));
                m.insert(
                    "tool_calls".into(),
                    serde_json::Value::Array(vec![serde_json::Value::Object(tool_call)]),
                );
                serde_json::Value::Object(m)
            }
            ChatMessage::ToolResult { id, content } => {
                let mut m = serde_json::Map::new();
                m.insert("role".into(), serde_json::Value::String("tool".into()));
                m.insert("tool_call_id".into(), serde_json::Value::String(id.clone()));
                m.insert("content".into(), serde_json::Value::String(content.clone()));
                serde_json::Value::Object(m)
            }
        })
        .collect()
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
