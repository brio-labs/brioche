//! Desktop settings persistence.
//!
//! Brioche 0.1 uses a module-scoped settings store. Each module (chat, context,
//! memory, ...) reads and writes values under a dotted key such as
//! `chat.model`. The frontend renders generic editors from the registered
//! [`crate::extensions::settings_sections::SettingsSection`] descriptors.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A configured AMP-compatible memory endpoint.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEndpoint {
    /// Machine-readable provider id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Base URL of the AMP server.
    pub url: String,
    /// Optional API key.
    pub api_key: Option<String>,
    /// Default scope.
    pub scope: Option<String>,
}

/// A fallback model definition.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FallbackModel {
    /// Provider identifier.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Optional API key override.
    pub api_key: Option<String>,
    /// Optional base URL override.
    pub base_url: Option<String>,
    /// Context window for this fallback.
    pub context_window: Option<u32>,
    /// Reasoning mode for this fallback.
    pub reasoning_enabled: Option<bool>,
    /// Reasoning effort for this fallback.
    pub reasoning_effort: Option<String>,
}

/// User-configurable settings for the desktop app.
///
/// Settings are stored as a JSON object so that modules can add values without
/// modifying this struct. Legacy flat fields are kept for backward compatibility
/// with the previous 0.0.1 settings file.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    /// Module-scoped settings values.
    #[serde(flatten)]
    pub modules: BTreeMap<String, Value>,
}

fn home_or_tmp() -> String {
    match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(v) => v,
        Err(_) => "/tmp".into(),
    }
}

impl Default for Settings {
    fn default() -> Self {
        let mut modules = BTreeMap::new();
        modules.insert(
            "chat".into(),
            Value::Object(
                [
                    ("provider".into(), Value::String("openrouter".into())),
                    ("model".into(), Value::String("qwen/qwen3.7-plus".into())),
                    ("api_key".into(), Value::String(String::new())),
                    (
                        "base_url".into(),
                        Value::String("https://openrouter.ai/api/v1".into()),
                    ),
                    ("max_tokens".into(), Value::Number(4096.into())),
                    (
                        "context_window".into(),
                        Value::Number(128_000.into()),
                    ),
                    ("reasoning_enabled".into(), Value::Bool(false)),
                    (
                        "reasoning_effort".into(),
                        Value::String("medium".into()),
                    ),
                    ("fallback_models".into(), Value::Array(Vec::new())),
                    ("personality".into(), Value::String("helpful".into())),
                    ("custom_identity".into(), Value::String(String::new())),
                    (
                        "system_prompt".into(),
                        Value::String(
                            "You are a helpful AI coding assistant with access to filesystem tools."
                                .into(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        );
        modules.insert(
            "context".into(),
            Value::Object(
                [
                    ("enabled".into(), Value::Bool(true)),
                    ("trigger_percentage".into(), Value::Number(75.into())),
                    ("target_percentage".into(), Value::Number(50.into())),
                    ("preserve_recent".into(), Value::Number(6.into())),
                ]
                .into_iter()
                .collect(),
            ),
        );
        modules.insert(
            "memory".into(),
            Value::Object(
                [
                    (
                        "active_providers".into(),
                        Value::Array(vec![Value::String("memory-local".into())]),
                    ),
                    (
                        "endpoints".into(),
                        Value::Array(vec![Value::Object(
                            [
                                ("id".into(), Value::String("memory-amp-1".into())),
                                ("name".into(), Value::String("Remote memory".into())),
                                (
                                    "url".into(),
                                    Value::String("http://localhost:9471".into()),
                                ),
                                ("api_key".into(), Value::Null),
                                ("scope".into(), Value::Null),
                            ]
                            .into_iter()
                            .collect(),
                        )]),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        );
        modules.insert(
            "ui".into(),
            Value::Object(
                [
                    ("working_dir".into(), Value::String(home_or_tmp())),
                    ("stream".into(), Value::Bool(true)),
                ]
                .into_iter()
                .collect(),
            ),
        );
        Self { modules }
    }
}

impl Settings {
    /// Loads settings from disk, or returns defaults if the file doesn't exist.
    pub fn load() -> Self {
        let path = settings_path();
        if let Ok(data) = std::fs::read_to_string(&path)
            && let Ok(mut settings) = serde_json::from_str::<Settings>(&data)
        {
            // Merge missing default module values so upgrades keep working.
            let defaults = Self::default();
            for (key, value) in defaults.modules {
                settings.modules.entry(key).or_insert(value);
            }
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
        std::fs::write(&path, data).map_err(|e| format!("Failed to write settings: {e}"))
    }

    /// Returns a module object, creating it with an empty object if missing.
    pub fn module(&self, name: &str) -> Value {
        match self.modules.get(name) {
            Some(v) => v.clone(),
            None => Value::Object(serde_json::Map::new()),
        }
    }

    /// Returns a dotted value such as `chat.model`.
    pub fn get(&self, key: &str) -> Option<Value> {
        let mut parts = key.split('.');
        let module = parts.next()?;
        let mut value = self.modules.get(module)?;
        for part in parts {
            value = value.get(part)?;
        }
        Some(value.clone())
    }

    /// Sets a dotted value such as `chat.model`.
    pub fn set(&mut self, key: &str, value: Value) -> Result<(), String> {
        let mut parts: Vec<&str> = key.split('.').collect();
        if parts.len() < 2 {
            return Err("Settings keys must be module.value".into());
        }
        let module = parts.remove(0);
        let module_value = self
            .modules
            .entry(module.to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let mut current = module_value
            .as_object_mut()
            .ok_or_else(|| format!("Module '{module}' is not an object"))?;
        while parts.len() > 1 {
            let part = parts.remove(0);
            current = current
                .entry(part)
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .ok_or_else(|| format!("'{part}' is not an object"))?;
        }
        current.insert(parts[0].to_string(), value);
        Ok(())
    }

    /// Returns the working directory.
    pub fn working_dir(&self) -> String {
        match self.get("ui.working_dir") {
            Some(Value::String(s)) => s,
            _ => home_or_tmp(),
        }
    }

    /// Returns the working directory as a PathBuf.
    pub fn working_dir_path(&self) -> PathBuf {
        PathBuf::from(self.working_dir())
    }

    /// Returns the active provider for chat.
    pub fn chat_provider(&self) -> String {
        match self.get("chat.provider") {
            Some(Value::String(s)) => s,
            _ => "openrouter".into(),
        }
    }

    /// Returns the active chat model.
    pub fn chat_model(&self) -> String {
        match self.get("chat.model") {
            Some(Value::String(s)) => s,
            _ => "qwen/qwen3.7-plus".into(),
        }
    }

    /// Returns the configured API key.
    pub fn api_key(&self) -> String {
        match self.get("chat.api_key") {
            Some(Value::String(s)) => s,
            _ => String::new(),
        }
    }

    /// Returns the configured base URL.
    pub fn base_url(&self) -> String {
        match self.get("chat.base_url") {
            Some(Value::String(s)) => s,
            _ => "https://openrouter.ai/api/v1".into(),
        }
    }

    /// Returns the configured max tokens.
    pub fn max_tokens(&self) -> u32 {
        match self.get("chat.max_tokens") {
            Some(Value::Number(n)) => n.as_u64().map_or(4096, |v| v as u32),
            _ => 4096,
        }
    }

    /// Returns the configured context window.
    pub fn context_window(&self) -> usize {
        match self.get("chat.context_window") {
            Some(Value::Number(n)) => n.as_u64().map_or(128_000, |v| v as usize),
            _ => 128_000,
        }
    }

    /// Returns whether streaming is enabled.
    pub fn stream(&self) -> bool {
        match self.get("ui.stream") {
            Some(Value::Bool(b)) => b,
            _ => true,
        }
    }

    /// Returns the active memory provider ids.
    pub fn active_memory_providers(&self) -> Vec<String> {
        match self.get("memory.active_providers") {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => vec!["memory-local".into()],
        }
    }

    /// Returns configured AMP-compatible memory endpoints.
    pub fn memory_endpoints(&self) -> Vec<MemoryEndpoint> {
        match self.get("memory.endpoints") {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| serde_json::from_value::<MemoryEndpoint>(v.clone()).ok())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Returns fallback models.
    pub fn fallback_models(&self) -> Vec<FallbackModel> {
        self.get("chat.fallback_models").map_or(Vec::new(), |v| {
            serde_json::from_value::<Vec<FallbackModel>>(v).map_or(Vec::new(), |m| m)
        })
    }

    /// Returns the current system prompt.
    pub fn system_prompt(&self) -> String {
        match self.get("chat.system_prompt") {
            Some(Value::String(s)) => s,
            _ => "You are a helpful AI coding assistant with access to filesystem tools.".into(),
        }
    }

    /// Returns the current personality.
    pub fn personality(&self) -> String {
        match self.get("chat.personality") {
            Some(Value::String(s)) => s,
            _ => "helpful".into(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_get_set_dotted() {
        let mut settings = Settings::default();
        assert!(
            settings
                .set("chat.model", Value::String("gpt-4o".into()))
                .is_ok(),
            "set should succeed"
        );
        assert_eq!(
            settings.get("chat.model"),
            Some(Value::String("gpt-4o".into()))
        );
    }

    #[test]
    fn settings_default_model() {
        let settings = Settings::default();
        assert_eq!(settings.chat_model(), "qwen/qwen3.7-plus");
    }
}
