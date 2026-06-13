//! `agent-terminal` — Minimal terminal agent for Brioche.
//!
//! Minimal entry point: argument parsing, persistence initialization,
//! and dispatch to headless or interactive mode.
//!
//! Agent-specific modules:
//! - `shell_builder` — builds a complete `BriocheShell`
//! - `headless` — non-interactive mode (single command)
//! - `interactive` — REPL with multi-session support
//! - `bridge` — message routing and slash commands
//!
//! Shared terminal infrastructure (from `brioche-reedline`):
//! - `repl` — blocking read via reedline
//! - `session` — multi-session manager
//! - `ui` — terminal rendering
//!
//! Refs: SPECS.md §Book III-A, §Book III-C

pub mod bridge;
pub mod headless;
pub mod interactive;
pub mod shell_builder;

// ---------------------------------------------------------------------------
// CLI configuration (merged from config.rs)
// ---------------------------------------------------------------------------

use brioche_provider_openai::OpenAiConfig;

/// Global CLI configuration.
/// Refs: SPECS.md §Book IV
#[derive(Clone, Debug)]
pub struct CliConfig {
    /// OpenAI provider configuration.
    pub openai: OpenAiConfig,
    /// Tick interval in milliseconds.
    pub tick_interval_ms: u64,
}

/// User-provided configuration source (CLI args).
/// Refs: SPECS.md §Book IV
#[derive(Clone, Debug, Default)]
pub struct UserConfig {
    /// API key for the LLM provider.
    pub api_key: Option<String>,
    /// LLM model identifier.
    pub model: Option<String>,
    /// Base URL for the API endpoint.
    pub base_url: Option<String>,
}

impl CliConfig {
    /// Builds the config by merging env vars + CLI args.
    ///
    /// Environment variables:
    /// - `BRIOCHE_API_KEY` — API key
    /// - `BRIOCHE_MODEL` — model (default: gpt-4o-mini)
    /// - `BRIOCHE_BASE_URL` — endpoint (default: <https://api.openai.com/v1>)
    ///
    /// Refs: SPECS.md §Book IV
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
