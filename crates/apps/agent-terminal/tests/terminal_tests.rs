//! Tests for agent-terminal CLI infrastructure.
//!
//! Covers `CliConfig`, `UserConfig`, and `print_repl_help`.

use agent_terminal::bridge::print_repl_help;
use agent_terminal::{CliConfig, UserConfig};

#[test]
fn cli_config_from_env_and_args_uses_defaults() {
    let user = UserConfig::default();
    let config = CliConfig::from_env_and_args(user);
    assert_eq!(config.openai.model, "gpt-4o-mini");
    assert_eq!(config.openai.base_url, "https://api.openai.com/v1");
    assert_eq!(config.tick_interval_ms, 1000);
}

#[test]
fn cli_config_from_env_and_args_uses_user_values() {
    let user = UserConfig {
        api_key: Some("test-key".to_string()),
        model: Some("gpt-4o".to_string()),
        base_url: Some("https://custom.example.com/v1".to_string()),
    };
    let config = CliConfig::from_env_and_args(user);
    assert_eq!(config.openai.api_key, "test-key");
    assert_eq!(config.openai.model, "gpt-4o");
    assert_eq!(config.openai.base_url, "https://custom.example.com/v1");
}

#[test]
fn print_repl_help_contains_commands() {
    let help = print_repl_help();
    assert!(help.contains("/help"));
    assert!(help.contains("/quit"));
    assert!(help.contains("/session"));
    assert!(help.contains("BRIOCHE_API_KEY"));
}

#[test]
fn print_repl_help_contains_shortcuts() {
    let help = print_repl_help();
    assert!(help.contains("Ctrl+C"));
    assert!(help.contains("Ctrl+D"));
}

#[test]
fn user_config_default_is_empty() {
    let user = UserConfig::default();
    assert!(user.api_key.is_none());
    assert!(user.model.is_none());
    assert!(user.base_url.is_none());
}
