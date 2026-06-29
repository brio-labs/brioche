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
//! Refs: docs/SPECS.md §Book III-A, §Book III-C

pub mod bridge;
pub mod headless;
pub mod interactive;
pub mod shell_builder;

// ---------------------------------------------------------------------------
// CLI configuration (merged from config.rs)
// ---------------------------------------------------------------------------

use brioche_provider_openai::OpenAiConfig;
use brioche_shell_builder::assemble_openai_config_from_env;

/// Global CLI configuration.
/// Refs: docs/SPECS.md §Book IV
#[derive(Clone, Debug)]
pub struct CliConfig {
    /// OpenAI provider configuration.
    pub openai: OpenAiConfig,
    /// Tick interval in milliseconds.
    pub tick_interval_ms: u64,
}

/// User-provided configuration source (CLI args).
/// Refs: docs/SPECS.md §Book IV
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
    /// Refs: docs/SPECS.md §Book IV
    pub fn from_env_and_args(user: UserConfig) -> Self {
        let openai = assemble_openai_config_from_env(user.api_key, user.model, user.base_url);

        Self {
            openai,
            tick_interval_ms: 1000,
        }
    }
}
