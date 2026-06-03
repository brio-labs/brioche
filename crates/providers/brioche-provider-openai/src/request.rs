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

/// Maximum characters to keep per tool result message.
///
/// Tool results (e.g. `read_file`, `list_dir`) can be very large.
/// Keeping the full output in the LLM history quickly exhausts
/// the context window. 4000 chars ≈ 1000 tokens (heuristic).
const MAX_TOOL_RESULT_CHARS: usize = 4000;

/// Maximum messages to send in a single request.
///
/// Keeps the most recent messages, plus the system message.
/// This prevents context window overflow on long sessions.
const MAX_MESSAGES_PER_REQUEST: usize = 30;

/// Truncates a string with an ellipsis marker if it exceeds `max_len`.
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_len - 3).collect();
        result.push_str("...");
        result
    }
}

/// Converts a `ToolCallDescriptor` into the OpenAI `tool_calls` object.
fn openai_tool_call(id: &str, name: &str, arguments: &str) -> serde_json::Value {
    let mut func = serde_json::Map::new();
    func.insert("name".into(), serde_json::Value::String(name.into()));
    func.insert(
        "arguments".into(),
        serde_json::Value::String(arguments.into()),
    );

    let mut tool_call = serde_json::Map::new();
    tool_call.insert("id".into(), serde_json::Value::String(id.into()));
    tool_call.insert("type".into(), serde_json::Value::String("function".into()));
    tool_call.insert("function".into(), serde_json::Value::Object(func));
    serde_json::Value::Object(tool_call)
}

/// Converts Brioche history into an array of OpenAI messages.
///
/// `Assistant` messages carry their `tool_calls` inline, matching
/// the OpenAI wire format directly. No merge logic is needed
/// because the kernel produces the correct structure.
///
/// Long tool results are truncated to `MAX_TOOL_RESULT_CHARS`.
/// If history exceeds `MAX_MESSAGES_PER_REQUEST`, only the
/// system message (if any) and the most recent messages are kept.
///
/// # Complexity
/// O(n) where n = number of messages. One `Vec` allocation.
pub fn build_messages(history: &[ChatMessage]) -> Vec<serde_json::Value> {
    let (start_idx, keep_first) = if history.len() > MAX_MESSAGES_PER_REQUEST {
        let first_is_system = matches!(&history.first(), Some(ChatMessage::System { .. }));
        if first_is_system {
            (history.len() - (MAX_MESSAGES_PER_REQUEST - 1), true)
        } else {
            (history.len() - MAX_MESSAGES_PER_REQUEST, false)
        }
    } else {
        (0, false)
    };

    let slice = &history[start_idx..];
    let mut result = Vec::with_capacity(slice.len() + if keep_first { 1 } else { 0 });

    if keep_first && let ChatMessage::System { content } = &history[0] {
        result.push(simple_msg("system", content));
    }

    for msg in slice {
        match msg {
            ChatMessage::System { content } => {
                result.push(simple_msg("system", content));
            }
            ChatMessage::User { content } => {
                result.push(simple_msg("user", content));
            }
            ChatMessage::Assistant {
                content,
                reasoning,
                tool_calls,
            } => {
                let mut m = serde_json::Map::new();
                m.insert("role".into(), serde_json::Value::String("assistant".into()));
                m.insert("content".into(), serde_json::Value::String(content.clone()));

                if !tool_calls.is_empty() {
                    let tc = tool_calls
                        .iter()
                        .map(|tc| openai_tool_call(&tc.tool_id, &tc.tool_name, &tc.arguments))
                        .collect();
                    m.insert("tool_calls".into(), serde_json::Value::Array(tc));
                }

                if let Some(r) = reasoning
                    && !r.is_empty()
                {
                    m.insert("reasoning".into(), serde_json::Value::String(r.clone()));
                }

                result.push(serde_json::Value::Object(m));
            }
            ChatMessage::ToolRequest {
                id,
                name,
                arguments,
            } => {
                // Backward-compat: standalone ToolRequest not preceded by
                // an Assistant (should not happen with current kernel).
                let mut m = serde_json::Map::new();
                m.insert("role".into(), serde_json::Value::String("assistant".into()));
                m.insert("content".into(), serde_json::Value::String("".into()));
                m.insert(
                    "tool_calls".into(),
                    serde_json::Value::Array(vec![openai_tool_call(id, name, arguments)]),
                );
                result.push(serde_json::Value::Object(m));
            }
            ChatMessage::ToolResult { id, content } => {
                let trimmed = truncate(content, MAX_TOOL_RESULT_CHARS);
                let mut m = serde_json::Map::new();
                m.insert("role".into(), serde_json::Value::String("tool".into()));
                m.insert("tool_call_id".into(), serde_json::Value::String(id.clone()));
                m.insert("content".into(), serde_json::Value::String(trimmed));
                result.push(serde_json::Value::Object(m));
            }
        }
    }
    result
}

/// Assembles the Chat Completions request body.
///
/// `tools` is optional. When provided, the `tools` field is injected
/// into the payload to enable function calling mode.
pub fn build_request_body(
    model: &str,
    messages: Vec<serde_json::Value>,
    max_tokens: u32,
    reasoning_effort: Option<&str>,
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

    if let Some(effort) = reasoning_effort {
        let mut reasoning = serde_json::Map::new();
        reasoning.insert("effort".into(), serde_json::Value::String(effort.into()));
        body.insert("reasoning".into(), serde_json::Value::Object(reasoning));
    }

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
