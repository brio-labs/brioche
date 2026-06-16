//! Desktop settings persistence.
//!
//! Settings are stored as JSON in the user's config directory:
//! - Linux:   ~/.config/brioche-desktop/settings.json
//! - macOS:   ~/Library/Application Support/brioche-desktop/settings.json
//! - Windows: %APPDATA%\brioche-desktop\settings.json
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// User-configurable settings for the desktop app.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// OpenAI-compatible API key.
    pub api_key: String,
    /// LLM model identifier.
    pub model: String,
    /// API base URL (for OpenRouter, local proxies, etc.).
    pub base_url: String,
    /// Working / project directory for file operations.
    pub working_dir: String,
    /// Whether to stream responses.
    pub stream: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            working_dir: match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
                Ok(v) => v,
                Err(_) => "/tmp".into(),
            },
            stream: true,
        }
    }
}

impl Settings {
    /// Loads settings from disk, or returns defaults if the file doesn't exist.
    pub fn load() -> Self {
        let path = settings_path();
        if let Ok(data) = std::fs::read_to_string(&path)
            && let Ok(settings) = serde_json::from_str::<Settings>(&data)
        {
            return settings;
        }
        Self::default()
    }

    /// Saves settings to disk.
    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {e}"))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize settings: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write settings: {e}"))?;
        Ok(())
    }

    /// Returns the working directory as a PathBuf.
    pub fn working_dir_path(&self) -> PathBuf {
        PathBuf::from(&self.working_dir)
    }
}

/// Returns the platform-appropriate settings file path.
fn settings_path() -> PathBuf {
    let config_dir = match dirs::config_dir() {
        Some(d) => d,
        None => std::env::temp_dir(),
    };
    config_dir.join("brioche-desktop").join("settings.json")
}
