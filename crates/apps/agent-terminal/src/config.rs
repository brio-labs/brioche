//! Brioche CLI configuration.
//!
//! Reads environment variables, CLI arguments, and the optional
//! `~/.config/brioche/cli.toml` file.
//!
//! Priority (highest wins):
//! 1. CLI arguments (--api-key, --model, --base-url)
//! 2. Environment variables (BRIOCHE_API_KEY, ...)
//! 3. Defaults
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_provider_openai::OpenAiConfig;

/// Global CLI configuration.
#[derive(Clone, Debug)]
pub struct CliConfig {
    pub openai: OpenAiConfig,
    pub tick_interval_ms: u64,
}

/// User-provided configuration source (CLI args).
#[derive(Clone, Debug, Default)]
pub struct UserConfig {
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

impl CliConfig {
    /// Builds the config by merging env vars + CLI args.
    ///
    /// Environment variables:
    /// - `BRIOCHE_API_KEY` — API key
    /// - `BRIOCHE_MODEL` — model (default: gpt-4o-mini)
    /// - `BRIOCHE_BASE_URL` — endpoint (default: <https://api.openai.com/v1>)
    pub fn from_env_and_args(user: UserConfig) -> Self {
        let api_key = user
            .api_key
            .or_else(|| std::env::var("BRIOCHE_API_KEY").ok())
            .map_or(String::new(), |v| v);
        let model = user
            .model
            .or_else(|| std::env::var("BRIOCHE_MODEL").ok())
            .map_or("gpt-4o-mini".into(), |v| v);
        let base_url = user
            .base_url
            .or_else(|| std::env::var("BRIOCHE_BASE_URL").ok())
            .map_or("https://api.openai.com/v1".into(), |v| v);

        let max_tokens = std::env::var("BRIOCHE_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(4096u32, |v| v);

        // Reasoning effort: user override only. No per-model defaults.
        //
        // OpenRouter supports `reasoning.effort` ("none", "minimal",
        // "low", "medium", "high", "xhigh"). When unset, the provider
        // uses its default behavior.
        let reasoning_effort = std::env::var("BRIOCHE_REASONING_EFFORT").ok();

        let openai = OpenAiConfig {
            api_key,
            model,
            base_url,
            max_tokens,
            timeout_ms: 120_000,
            reasoning_effort,
        };

        Self {
            openai,
            tick_interval_ms: 1000,
        }
    }
}
