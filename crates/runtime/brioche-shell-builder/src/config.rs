//! OpenAI provider configuration assembly.
//!
//! Both `agent-terminal` and `brioche-desktop` need to build an
//! [`OpenAiConfig`] from layered sources (CLI args / settings / env vars).
//! The helpers here keep that assembly in one place.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_provider_openai::OpenAiConfig;
use brioche_shell_persistence::Settings;

/// Default model identifier used when no other source provides one.
///
/// Refs: I-Shell-Runtime-OnlyIO
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// Default API endpoint used when no other source provides one.
///
/// Refs: I-Shell-Runtime-OnlyIO
const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Default maximum tokens per response.
///
/// Refs: I-Shell-Runtime-OnlyIO
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// Default request timeout in milliseconds.
///
/// Refs: I-Shell-Runtime-OnlyIO
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Assembles an [`OpenAiConfig`] from optional CLI values plus environment
/// variable fallbacks.
///
/// Environment variables:
/// - `BRIOCHE_API_KEY` — API key
/// - `BRIOCHE_MODEL` — model (default: `gpt-4o-mini`)
/// - `BRIOCHE_BASE_URL` — endpoint (default: `https://api.openai.com/v1`)
/// - `BRIOCHE_MAX_TOKENS` — max tokens (default: `4096`)
/// - `BRIOCHE_REASONING_EFFORT` — optional reasoning effort
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) env var lookups and string allocations.
///
/// # Panic / Safety
/// Never panics.
pub fn assemble_openai_config_from_env(
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
) -> OpenAiConfig {
    let api_key = api_key
        .or_else(|| std::env::var("BRIOCHE_API_KEY").ok())
        .map_or(String::new(), |v| v);
    let model = model
        .or_else(|| std::env::var("BRIOCHE_MODEL").ok())
        .map_or(DEFAULT_MODEL.into(), |v| v);
    let base_url = base_url
        .or_else(|| std::env::var("BRIOCHE_BASE_URL").ok())
        .map_or(DEFAULT_BASE_URL.into(), |v| v);
    let max_tokens = std::env::var("BRIOCHE_MAX_TOKENS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(DEFAULT_MAX_TOKENS, |v| v);
    let reasoning_effort = std::env::var("BRIOCHE_REASONING_EFFORT").ok();

    OpenAiConfig {
        api_key,
        model,
        base_url,
        max_tokens,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        reasoning_effort,
    }
}

/// Assembles an [`OpenAiConfig`] from desktop settings, with environment
/// variables overriding settings values.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) env var lookups and settings field accesses.
///
/// # Panic / Safety
/// Never panics.
pub fn assemble_openai_config_from_settings(settings: &Settings) -> OpenAiConfig {
    let api_key = if settings.api_key().is_empty() {
        std::env::var("BRIOCHE_API_KEY").map_or(String::new(), |v| v)
    } else {
        settings.api_key()
    };
    let model = std::env::var("BRIOCHE_MODEL").map_or(settings.chat_model(), |v| v);
    let base_url = std::env::var("BRIOCHE_BASE_URL").map_or(settings.base_url(), |v| v);
    let max_tokens = settings.max_tokens();

    let reasoning_enabled = settings
        .get("chat.reasoning_enabled")
        .is_some_and(|v| v.as_bool().is_some_and(|b| b));
    let reasoning_effort = if reasoning_enabled {
        Some(
            settings
                .get("chat.reasoning_effort")
                .map_or("medium".into(), |v| {
                    v.as_str().map_or("medium".into(), |s| s.into())
                }),
        )
    } else {
        std::env::var("BRIOCHE_REASONING_EFFORT").ok()
    };

    OpenAiConfig {
        api_key,
        model,
        base_url,
        max_tokens,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        reasoning_effort,
    }
}
