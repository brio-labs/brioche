//! Explicit configuration for the OpenAI provider.
//!
//! No environment variables are read here. The assembler (CLI)
//! reads the environment and injects a constructed `OpenAiConfig`.
//!
//! Refs: I-Shell-Runtime-OnlyIO

/// OpenAI client configuration.
///
/// `base_url` allows targeting Ollama, OpenRouter, or any other
/// OpenAI-compatible endpoint.
///
/// # Invariants
/// - `model` and `api_key` are never empty after construction.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub max_tokens: u32,
    pub timeout_ms: u64,
    /// Reasoning effort level sent to the provider.
    ///
    /// OpenRouter normalizes this across providers:
    /// - `"none"` — disables reasoning entirely (useful for MiniMax M3
    ///   which emits creative output in reasoning instead of tools)
    /// - `"minimal"` / `"low"` / `"medium"` / `"high"` / `"xhigh"`
    ///
    /// When `None`, no reasoning parameter is sent and the provider
    /// uses its default behavior.
    ///
    /// See: openrouter.ai/docs/guides/best-practices/reasoning-tokens
    pub reasoning_effort: Option<String>,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            max_tokens: 4096,
            timeout_ms: 120_000,
            reasoning_effort: None,
        }
    }
}
