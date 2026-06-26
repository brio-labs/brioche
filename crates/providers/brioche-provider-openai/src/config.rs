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
/// Validation error for `OpenAiConfig`.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OpenAiConfigError {
    /// `api_key` is empty.
    #[error("api_key must not be empty")]
    EmptyApiKey,
    /// `model` is empty.
    #[error("model must not be empty")]
    EmptyModel,
}

/// OpenAI client configuration.
///
/// `base_url` allows targeting Ollama, OpenRouter, or any other
/// OpenAI-compatible endpoint.
///
/// # Invariants
/// - `model` and `api_key` are validated before use.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAiConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Model identifier (e.g. `gpt-4o-mini`).
    pub model: String,
    /// Base URL of the OpenAI-compatible endpoint.
    pub base_url: String,
    /// Maximum tokens per generation.
    pub max_tokens: u32,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Reasoning effort level sent to the provider.
    ///
    /// OpenRouter normalizes this across providers:
    /// - `"none"` — disables reasoning entirely (useful for MiniMax M3
    ///   which emits creative output in reasoning instead of tools)
    /// - `"minimal"` / `"low"` / `"medium"` / `"high"` / `"xhigh"`
    ///
    /// When `None`, no reasoning parameter is sent and the provider
    /// uses its default.
    ///
    /// See: openrouter.ai/docs/guides/best-practices/reasoning-tokens
    pub reasoning_effort: Option<String>,
}

impl OpenAiConfig {
    /// Validates the configuration.
    ///
    /// Returns `Ok(())` if `api_key` and `model` are non-empty.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn validate(&self) -> Result<(), OpenAiConfigError> {
        if self.api_key.is_empty() {
            return Err(OpenAiConfigError::EmptyApiKey);
        }
        if self.model.is_empty() {
            return Err(OpenAiConfigError::EmptyModel);
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_api_key() {
        let config = OpenAiConfig {
            api_key: String::new(),
            ..OpenAiConfig::default()
        };
        assert_eq!(config.validate(), Err(OpenAiConfigError::EmptyApiKey));
    }

    #[test]
    fn rejects_empty_model() {
        let config = OpenAiConfig {
            api_key: "sk-test".into(),
            model: String::new(),
            ..OpenAiConfig::default()
        };
        assert_eq!(config.validate(), Err(OpenAiConfigError::EmptyModel));
    }
    #[test]
    fn accepts_nonempty_api_key_and_model() {
        let config = OpenAiConfig {
            api_key: "sk-test".into(),
            model: "gpt-4o-mini".into(),
            ..OpenAiConfig::default()
        };
        assert!(config.validate().is_ok());
    }
}
