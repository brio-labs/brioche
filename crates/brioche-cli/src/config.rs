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
            .unwrap_or_default();
        let model = user
            .model
            .or_else(|| std::env::var("BRIOCHE_MODEL").ok())
            .unwrap_or_else(|| "gpt-4o-mini".into());
        let base_url = user
            .base_url
            .or_else(|| std::env::var("BRIOCHE_BASE_URL").ok())
            .unwrap_or_else(|| "https://api.openai.com/v1".into());

        let openai = OpenAiConfig {
            api_key,
            model,
            base_url,
            max_tokens: 4096,
            timeout_ms: 120_000,
        };

        Self {
            openai,
            tick_interval_ms: 1000,
        }
    }
}
