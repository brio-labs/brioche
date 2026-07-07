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
    /// `base_url` is not a valid HTTP(S) URL.
    #[error("base_url is not a valid HTTP(S) URL: {0}")]
    InvalidBaseUrl(String),
    /// `base_url` points at a local endpoint without explicit opt-in.
    #[error("base_url host is not allowed: {0}")]
    BlockedBaseUrl(String),
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
    /// Allows loopback/private provider endpoints for explicitly configured local models.
    pub allow_loopback: bool,
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
        validate_base_url(&self.base_url, self.allow_loopback)?;
        Ok(())
    }
}

fn validate_base_url(base_url: &str, allow_loopback: bool) -> Result<(), OpenAiConfigError> {
    let parsed = reqwest::Url::parse(base_url)
        .map_err(|_| OpenAiConfigError::InvalidBaseUrl(base_url.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(OpenAiConfigError::InvalidBaseUrl(base_url.to_string()));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| OpenAiConfigError::InvalidBaseUrl(base_url.to_string()))?;
    if !allow_loopback && is_blocked_host(host) {
        return Err(OpenAiConfigError::BlockedBaseUrl(host.to_string()));
    }
    Ok(())
}

fn is_blocked_host(host: &str) -> bool {
    let normalized = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if matches!(normalized.as_str(), "localhost" | "localhost.localdomain") {
        return true;
    }
    match normalized.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            let first_octet = ip.octets().first().copied().map_or(0, |octet| octet);
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_unspecified()
                || first_octet == 0
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            let first_segment = ip.segments().first().copied().map_or(0, |segment| segment);
            ip.is_loopback() || ip.is_unspecified() || first_segment & 0xfe00 == 0xfc00
        }
        Err(_) => false,
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
            allow_loopback: false,
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
    fn rejects_file_base_url() {
        let config = OpenAiConfig {
            api_key: "sk-test".into(),
            base_url: "file:///tmp/provider".into(),
            ..OpenAiConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(OpenAiConfigError::InvalidBaseUrl(_))
        ));
    }

    #[test]
    fn rejects_loopback_base_url_without_opt_in() {
        let config = OpenAiConfig {
            api_key: "sk-test".into(),
            base_url: "http://localhost:11434/v1".into(),
            ..OpenAiConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(OpenAiConfigError::BlockedBaseUrl(_))
        ));
    }

    #[test]
    fn accepts_loopback_base_url_with_opt_in() {
        let config = OpenAiConfig {
            api_key: "sk-test".into(),
            base_url: "http://127.0.0.1:11434/v1".into(),
            allow_loopback: true,
            ..OpenAiConfig::default()
        };
        assert!(config.validate().is_ok());
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
