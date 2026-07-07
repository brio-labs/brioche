//! Redacted diagnostic request dumps for OpenAI-compatible clients.
//!
//! This module is the only provider-client code that writes diagnostic files.
//! `call_llm` gates it behind `BRIOCHE_DIAG`, and request content is redacted
//! before writing to the private cache directory.
//!
//! Refs: docs/SPECS.md §Book III-B

use std::path::PathBuf;

/// Maximum size of a redacted diagnostic request body, in bytes.
const MAX_DIAG_BYTES: usize = 1_048_576;

/// Diagnostic marker for redacted text fields.
const REDACTED: &str = "[REDACTED]";

/// Returns the private diagnostic directory, creating it with 0700 if needed.
///
/// Uses `$XDG_CACHE_HOME/brioche/diag` when available, otherwise
/// falls back to `$HOME/.cache/brioche/diag`.
fn private_diag_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                let mut path = PathBuf::from(home);
                path.push(".cache");
                path
            })
        })?;

    let mut dir = base;
    dir.push("brioche");
    dir.push("diag");

    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "failed to create diagnostic directory");
        return None;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&dir).ok()?;
        let mut perms = metadata.permissions();
        // Ensure the directory is not world-readable/searchable.
        let mode = perms.mode() & 0o777;
        if mode & 0o077 != 0 {
            perms.set_mode(0o700);
            if let Err(e) = std::fs::set_permissions(&dir, perms) {
                tracing::warn!(error = %e, "failed to set diagnostic directory permissions");
                return None;
            }
        }
    }

    Some(dir)
}

/// Recursively redact sensitive string fields from a request body.
///
/// Redacts `content` in messages and `description` in tool function
/// definitions. Leaves structural metadata intact for debugging.
fn redact_request_body(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let redacted = if k == "content" || k == "description" {
                    match v {
                        serde_json::Value::String(_) => serde_json::Value::String(REDACTED.into()),
                        _ => redact_request_body(v),
                    }
                } else {
                    redact_request_body(v)
                };
                out.insert(k.clone(), redacted);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(redact_request_body).collect())
        }
        other => other.clone(),
    }
}

/// Writes a redacted, size-capped request body to the private diagnostic dir.
///
/// Refs: I-Shell-Network-Signal
pub(super) fn write_diag_request(turn: usize, body: &serde_json::Value) {
    let Some(dir) = private_diag_dir() else {
        return;
    };

    let mut path = dir;
    path.push(format!("brioche_request_turn_{turn}.json"));

    let redacted = redact_request_body(body);
    let mut text = redacted.to_string();
    const TRUNCATION_SUFFIX: &str = "\n...[truncated]";
    if text.len() > MAX_DIAG_BYTES {
        let limit = MAX_DIAG_BYTES.saturating_sub(TRUNCATION_SUFFIX.len());
        let trunc_idx = text.floor_char_boundary(limit);
        text.truncate(trunc_idx);
        text.push_str(TRUNCATION_SUFFIX);
    }

    if let Err(e) = std::fs::write(&path, &text) {
        tracing::warn!(error = %e, path = %path.display(), "failed to write diagnostic request");
    }
}

#[cfg(test)]
mod diag_tests {
    use super::{REDACTED, redact_request_body};

    fn obj(entries: &[(&str, serde_json::Value)]) -> serde_json::Value {
        serde_json::Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn arr(values: &[serde_json::Value]) -> serde_json::Value {
        serde_json::Value::Array(values.to_vec())
    }

    fn s(value: &str) -> serde_json::Value {
        serde_json::Value::String(value.into())
    }

    #[test]
    fn redact_request_body_obscures_message_content() {
        let body = obj(&[
            ("model", s("gpt-4o")),
            (
                "messages",
                arr(&[
                    obj(&[
                        ("role", s("system")),
                        ("content", s("secret system prompt")),
                    ]),
                    obj(&[("role", s("user")), ("content", s("secret user message"))]),
                ]),
            ),
            (
                "tools",
                arr(&[obj(&[
                    ("type", s("function")),
                    (
                        "function",
                        obj(&[
                            ("name", s("read_file")),
                            ("description", s("secret tool description")),
                        ]),
                    ),
                ])]),
            ),
        ]);

        let redacted = redact_request_body(&body);
        assert_eq!(redacted["model"], s("gpt-4o"));
        assert_eq!(redacted["messages"][0]["content"], s(REDACTED));
        assert_eq!(redacted["messages"][1]["content"], s(REDACTED));
        assert_eq!(redacted["tools"][0]["function"]["description"], s(REDACTED));
        assert_eq!(redacted["tools"][0]["function"]["name"], s("read_file"));
    }

    #[test]
    fn redact_request_body_leaves_non_sensitive_values_intact() {
        let body = obj(&[
            ("model", s("gpt-4o")),
            ("stream", serde_json::Value::Bool(true)),
            ("max_tokens", serde_json::Value::Number(4096.into())),
        ]);

        let redacted = redact_request_body(&body);
        assert_eq!(redacted["model"], s("gpt-4o"));
        assert_eq!(redacted["stream"], serde_json::Value::Bool(true));
        assert_eq!(
            redacted["max_tokens"],
            serde_json::Value::Number(4096.into())
        );
    }
}
