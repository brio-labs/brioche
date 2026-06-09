//! Composable SSE delta extractors.
//!
//! Different providers and model families expose streaming text
//! through different JSON field names inside `choices[].delta`.
//! This module turns provider-specific protocol quirks into
//! composable, testable units instead of `if` branches in the
//! client hot loop.
//!
//! Refs: I-Eco-ExtensionOverMod

/// A text fragment extracted from an SSE `delta` object.
///
/// `is_reasoning` distinguishes internal chain-of-thought text
/// from actual assistant `content`. Reasoning text is displayed
/// to the user for transparency but is **not** persisted in the
/// conversation history.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug)]
pub struct ExtractedText {
    /// Extracted text fragment.
    pub text: String,
    /// When `true`, this text came from `delta.reasoning` or
    /// `delta.reasoning_content` and should not be added to the
    /// assistant's history message.
    pub is_reasoning: bool,
}

/// Extracts streaming text fragments from an SSE `delta` object.
///
/// Each extractor is a pure function from `&serde_json::Value`
/// (the `delta` field) to an optional `ExtractedText`. No I/O,
/// no side effects, no mutation.
///
/// # Invariants
/// - Deterministic: same `delta` always yields same `Option<ExtractedText>`.
///
/// # Complexity
/// O(1). One or two JSON pointer lookups.
pub trait ChunkExtractor: Send + Sync {
    /// Returns the text fragment contained in this delta, if any.
    ///
    /// The returned string is forwarded to the kernel as a
    /// `StreamEvent::TextChunk` and broadcast as `LlmChunk::Text`.
    fn extract_text(&self, delta: &serde_json::Value) -> Option<ExtractedText>;
}

/// Standard extractor for non-reasoning models.
///
/// Reads only `delta.content`. Works for GPT-4, GPT-4o, Claude
/// (via OpenRouter), Llama, Mistral, and most other models.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Copy, Debug, Default)]
pub struct StandardExtractor;

impl ChunkExtractor for StandardExtractor {
    fn extract_text(&self, delta: &serde_json::Value) -> Option<ExtractedText> {
        delta
            .get("content")
            .and_then(|c| c.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| ExtractedText {
                text: s.to_string(),
                is_reasoning: false,
            })
    }
}

/// Extractor for reasoning models.
///
/// Reasoning models may emit text in multiple fields:
/// - `delta.content` — the final answer text
/// - `delta.reasoning` — OpenRouter Qwen, etc.
/// - `delta.reasoning_content` — DeepSeek, etc.
///
/// This extractor concatenates reasoning and content in that
/// order. When both are present, reasoning appears first.
///
/// # Rationale
/// Users of reasoning models typically want to see the chain of
/// thought as it is generated.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Copy, Debug, Default)]
pub struct ReasoningModelExtractor;

impl ChunkExtractor for ReasoningModelExtractor {
    fn extract_text(&self, delta: &serde_json::Value) -> Option<ExtractedText> {
        // Prefer reasoning fields, fall back to content.
        if let Some(text) = delta
            .get("reasoning")
            .or_else(|| delta.get("reasoning_content"))
            .and_then(|r| r.as_str())
            .filter(|s| !s.is_empty())
        {
            return Some(ExtractedText {
                text: text.to_string(),
                is_reasoning: true,
            });
        }

        delta
            .get("content")
            .and_then(|c| c.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| ExtractedText {
                text: s.to_string(),
                is_reasoning: false,
            })
    }
}

/// Returns the appropriate extractor for a given model identifier.
///
/// The model string is the raw identifier as configured by the
/// user (e.g. `gpt-4o-mini`, `qwen/qwen3.6-35b-a3b`,
/// `deepseek-chat`). Matching is case-insensitive and uses
/// substring detection so provider prefixes (`qwen/...`) are
/// handled transparently.
///
/// # Rationale
/// Keeping the dispatch table in one place makes it trivial to
/// add new reasoning model families without touching the client
/// loop.
///
/// # Complexity
/// O(1). Constant-time string prefix checks.
pub fn chunk_extractor_for_model(model: &str) -> Box<dyn ChunkExtractor> {
    let lower = model.to_lowercase();
    if lower.contains("qwen") || lower.contains("deepseek") || lower.contains("minimax") {
        Box::new(ReasoningModelExtractor)
    } else {
        Box::new(StandardExtractor)
    }
}

/// Detects provider-specific errors embedded in SSE events.
///
/// Some providers (OpenRouter, certain MiniMax deployments) send
/// `{"error": {"code": 400, "message": "..."}}` as a regular SSE
/// event instead of closing the stream with an HTTP error. Other
/// providers (OpenAI, Anthropic) never do this.
///
/// A `StreamErrorDetector` is a pure function from `&serde_json::Value`
/// (a parsed SSE event) to an optional `String` error message. No I/O,
/// no side effects.
///
/// # Invariants
/// - Deterministic: same event always yields same `Option<String>`.
///
/// # Complexity
/// O(1). One JSON pointer lookup.
pub trait StreamErrorDetector: Send + Sync {
    /// Returns an error message if this SSE event contains a provider
    /// error, or `None` if the event is normal.
    fn detect_error(&self, event: &serde_json::Value) -> Option<String>;
}

/// Detector for providers that do **not** embed errors in SSE events.
///
/// Always returns `None`. Used for OpenAI, Anthropic, and other
/// well-behaved providers.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoOpErrorDetector;

impl StreamErrorDetector for NoOpErrorDetector {
    fn detect_error(&self, _event: &serde_json::Value) -> Option<String> {
        None
    }
}

/// Detector for OpenRouter and similar providers that send mid-stream
/// errors.
///
/// Checks for `event.error.code` and `event.error.message`.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenRouterErrorDetector;

impl StreamErrorDetector for OpenRouterErrorDetector {
    fn detect_error(&self, event: &serde_json::Value) -> Option<String> {
        let error = event.get("error")?;
        let code = error.get("code").and_then(|c| c.as_u64()).unwrap_or(0);
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown provider error");
        Some(format!("Provider error (code {code}): {message}"))
    }
}

/// Returns the appropriate error detector for a given model identifier.
///
/// Models served through OpenRouter (prefixed with `qwen/`,
/// `minimax/`, `deepseek/`, etc.) use `OpenRouterErrorDetector`.
/// Known direct-provider models use `NoOpErrorDetector`.
///
/// # Complexity
/// O(1). Constant-time string prefix checks.
pub fn error_detector_for_model(model: &str) -> Box<dyn StreamErrorDetector> {
    let lower = model.to_lowercase();
    // OpenRouter prefixes: anything with a slash is likely OpenRouter.
    // Also catch known reasoning-model families that are typically
    // accessed via OpenRouter.
    if lower.contains('/') || lower.contains("openrouter") {
        Box::new(OpenRouterErrorDetector)
    } else {
        Box::new(NoOpErrorDetector)
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! uses unwrap internally
#[allow(clippy::unwrap_used)] // tests use unwrap for brevity
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn standard_extractor_content_only() {
        let delta = json!({ "content": "hello" });
        let extracted = StandardExtractor.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "hello");
        assert!(!extracted.is_reasoning);
    }

    #[test]
    fn standard_extractor_ignores_reasoning() {
        let delta = json!({ "reasoning": "think", "content": "hello" });
        // Standard extractor ignores reasoning fields
        let extracted = StandardExtractor.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "hello");
        assert!(!extracted.is_reasoning);
    }

    #[test]
    fn reasoning_extractor_prefers_reasoning() {
        let delta = json!({ "reasoning": "think", "content": "hello" });
        let extracted = ReasoningModelExtractor.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "think");
        assert!(extracted.is_reasoning);
    }

    #[test]
    fn reasoning_extractor_falls_back_to_content() {
        let delta = json!({ "content": "hello" });
        let extracted = ReasoningModelExtractor.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "hello");
        assert!(!extracted.is_reasoning);
    }

    #[test]
    fn reasoning_extractor_reasoning_content_field() {
        let delta = json!({ "reasoning_content": "deepthink" });
        let extracted = ReasoningModelExtractor.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "deepthink");
        assert!(extracted.is_reasoning);
    }

    #[test]
    fn dispatch_qwen() {
        let ex = chunk_extractor_for_model("qwen/qwen3.6-35b-a3b");
        let delta = json!({ "reasoning": "think" });
        let extracted = ex.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "think");
        assert!(extracted.is_reasoning);
    }

    #[test]
    fn dispatch_deepseek() {
        let ex = chunk_extractor_for_model("deepseek-chat");
        let delta = json!({ "reasoning_content": "deepthink" });
        let extracted = ex.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "deepthink");
        assert!(extracted.is_reasoning);
    }

    #[test]
    fn dispatch_gpt4o() {
        let ex = chunk_extractor_for_model("gpt-4o-mini");
        let delta = json!({ "content": "hi" });
        let extracted = ex.extract_text(&delta).unwrap();
        assert_eq!(extracted.text, "hi");
        assert!(!extracted.is_reasoning);
    }

    #[test]
    fn noop_detector_never_fires() {
        let detector = NoOpErrorDetector;
        let event = json!({ "error": { "code": 400, "message": "bad" } });
        assert!(detector.detect_error(&event).is_none());
    }

    #[test]
    fn openrouter_detector_fires_on_error() {
        let detector = OpenRouterErrorDetector;
        let event = json!({ "error": { "code": 400, "message": "invalid params" } });
        let err = detector.detect_error(&event).unwrap();
        assert!(err.contains("400"));
        assert!(err.contains("invalid params"));
    }

    #[test]
    fn openrouter_detector_ignores_normal_event() {
        let detector = OpenRouterErrorDetector;
        let event = json!({ "choices": [{ "delta": { "content": "hi" } }] });
        assert!(detector.detect_error(&event).is_none());
    }

    #[test]
    fn error_detector_dispatch_openrouter() {
        let d = error_detector_for_model("minimax/minimax-m3");
        let event = json!({ "error": { "code": 400, "message": "bad" } });
        assert!(d.detect_error(&event).is_some());
    }

    #[test]
    fn error_detector_dispatch_direct() {
        let d = error_detector_for_model("gpt-4o-mini");
        let event = json!({ "error": { "code": 400, "message": "bad" } });
        assert!(d.detect_error(&event).is_none());
    }
}
