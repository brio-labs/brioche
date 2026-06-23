//! Desktop settings persistence.
//!
//! Brioche 0.1 uses a module-scoped settings store. Each module (chat, context,
//! memory, ...) reads and writes values under a dotted key such as
//! `chat.model`. The frontend renders generic editors from the registered
//! [`crate::extensions::settings_sections::SettingsSection`] descriptors.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A configured AMP-compatible memory endpoint.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) representation of endpoint.
///
/// # Panic / Safety
/// Never panics.
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
///
/// # Complexity
/// O(1) representation of fallback.
///
/// # Panic / Safety
/// Never panics.
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
///
/// # Complexity
/// Holds settings map. Reads/writes are logarithmic in the number of keys.
///
/// # Panic / Safety
/// Never panics.
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
                                ("url".into(), Value::String("http://localhost:9471".into())),
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
            "tools".into(),
            Value::Object(
                [("user_tools_enabled".into(), Value::Bool(false))]
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the configuration file size. Performs blocking disk read.
    ///
    /// # Panic / Safety
    /// Never panics. Returns defaults if file reading or parsing fails.
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the serialized configuration size. Performs blocking disk write.
    ///
    /// # Panic / Safety
    /// Never panics. Returns error string if serialization or write fails.
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log M) where M is the number of configuration modules.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn module(&self, name: &str) -> Value {
        match self.modules.get(name) {
            Some(v) => v.clone(),
            None => Value::Object(serde_json::Map::new()),
        }
    }

    /// Returns a dotted value such as `chat.model`.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(D * log M) where D is the depth of the dot-path and M is the number of keys.
    ///
    /// # Panic / Safety
    /// Never panics.
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(D * log M) where D is the depth of the dot-path and M is the number of keys.
    ///
    /// # Panic / Safety
    /// Returns error string if intermediate path components are not JSON objects.
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
        let last_part = parts.first().ok_or_else(|| "Empty path".to_string())?;
        current.insert(last_part.to_string(), value);
        Ok(())
    }

    /// Returns the working directory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn working_dir(&self) -> String {
        match self.get("ui.working_dir") {
            Some(Value::String(s)) => s,
            _ => home_or_tmp(),
        }
    }

    /// Returns the working directory as a PathBuf.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn working_dir_path(&self) -> PathBuf {
        PathBuf::from(self.working_dir())
    }

    /// Returns the active provider for chat.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn chat_provider(&self) -> String {
        match self.get("chat.provider") {
            Some(Value::String(s)) => s,
            _ => "openrouter".into(),
        }
    }

    /// Returns the active chat model.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn chat_model(&self) -> String {
        match self.get("chat.model") {
            Some(Value::String(s)) => s,
            _ => "qwen/qwen3.7-plus".into(),
        }
    }

    /// Returns the configured API key.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn api_key(&self) -> String {
        match self.get("chat.api_key") {
            Some(Value::String(s)) => s,
            _ => String::new(),
        }
    }

    /// Returns the configured base URL.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn base_url(&self) -> String {
        match self.get("chat.base_url") {
            Some(Value::String(s)) => s,
            _ => "https://openrouter.ai/api/v1".into(),
        }
    }

    /// Returns the configured max tokens.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn max_tokens(&self) -> u32 {
        match self.get("chat.max_tokens") {
            Some(Value::Number(n)) => n.as_u64().map_or(4096, |v| v as u32),
            _ => 4096,
        }
    }

    /// Returns the configured context window.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn context_window(&self) -> usize {
        match self.get("chat.context_window") {
            Some(Value::Number(n)) => n.as_u64().map_or(128_000, |v| v as usize),
            _ => 128_000,
        }
    }

    /// Returns whether streaming is enabled.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn stream(&self) -> bool {
        match self.get("ui.stream") {
            Some(Value::Bool(b)) => b,
            _ => true,
        }
    }

    /// Returns the active memory provider ids.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn fallback_models(&self) -> Vec<FallbackModel> {
        self.get("chat.fallback_models").map_or(Vec::new(), |v| {
            serde_json::from_value::<Vec<FallbackModel>>(v).map_or(Vec::new(), |m| m)
        })
    }

    /// Returns the current system prompt.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn system_prompt(&self) -> String {
        match self.get("chat.system_prompt") {
            Some(Value::String(s)) => s,
            _ => "You are a helpful AI coding assistant with access to filesystem tools.".into(),
        }
    }

    /// Returns the current personality.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn personality(&self) -> String {
        match self.get("chat.personality") {
            Some(Value::String(s)) => s,
            _ => "helpful".into(),
        }
    }

    /// Validates settings before they are persisted or applied.
    ///
    /// Returns `Ok(())` if all configured values are acceptable, otherwise
    /// returns a newline-separated list of problems. Validation is conservative:
    /// it rejects values that would break the shell build or the LLM request.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO, I-Shell-Error-Propagate
    ///
    /// # Complexity
    /// O(E) where E is the number of configured memory endpoints.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn validate(&self) -> Result<(), String> {
        let mut errors = Vec::new();

        if self.chat_model().trim().is_empty() {
            errors.push("chat.model must not be empty".into());
        }

        let base_url = self.base_url();
        if base_url.trim().is_empty() {
            errors.push("chat.base_url must not be empty".into());
        } else if reqwest::Url::parse(&base_url).is_err() {
            errors.push(format!("chat.base_url is not a valid URL: {base_url}"));
        }

        if self.max_tokens() == 0 {
            errors.push("chat.max_tokens must be greater than 0".into());
        }
        if self.context_window() == 0 {
            errors.push("chat.context_window must be greater than 0".into());
        }

        let trigger = self
            .get("context.trigger_percentage")
            .and_then(|v| v.as_u64());
        let target = self
            .get("context.target_percentage")
            .and_then(|v| v.as_u64());
        if trigger.is_some_and(|n| n > 100) {
            errors.push("context.trigger_percentage must be between 0 and 100".into());
        }
        if target.is_some_and(|n| n > 100) {
            errors.push("context.target_percentage must be between 0 and 100".into());
        }
        if let (Some(t), Some(u)) = (trigger, target)
            && u > t
        {
            errors.push(
                "context.target_percentage must not exceed context.trigger_percentage".into(),
            );
        }

        for (index, endpoint) in self.memory_endpoints().iter().enumerate() {
            if endpoint.id.trim().is_empty() {
                errors.push(format!("memory.endpoints[{index}] is missing an id"));
            }
            if endpoint.name.trim().is_empty() {
                errors.push(format!("memory.endpoints[{index}] is missing a name"));
            }
            let url = endpoint.url.trim();
            if url.is_empty() {
                errors.push(format!("memory.endpoints[{index}] is missing a url"));
            } else if reqwest::Url::parse(url).is_err() {
                errors.push(format!(
                    "memory.endpoints[{index}].url is not a valid URL: {url}"
                ));
            }
        }

        if self.working_dir().trim().is_empty() {
            errors.push("ui.working_dir must not be empty".into());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }

    /// Returns whether user-defined tools are enabled.
    ///
    /// User-defined tools execute arbitrary shell commands or HTTP requests and
    /// are disabled by default for safety.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn user_tools_enabled(&self) -> bool {
        match self.get("tools.user_tools_enabled") {
            Some(Value::Bool(b)) => b,
            _ => false,
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
    fn user_tools_disabled_by_default() {
        let settings = Settings::default();
        assert!(
            !settings.user_tools_enabled(),
            "user-defined tools must be disabled by default"
        );
    }

    #[test]
    fn settings_default_model() {
        let settings = Settings::default();
        assert_eq!(settings.chat_model(), "qwen/qwen3.7-plus");
    }

    #[test]
    fn default_settings_validate() {
        let settings = Settings::default();
        assert!(
            settings.validate().is_ok(),
            "default settings should be valid"
        );
    }

    #[test]
    fn empty_model_is_invalid() {
        let mut settings = Settings::default();
        assert!(
            settings
                .set("chat.model", Value::String("".into()))
                .is_ok(),
            "set should succeed"
        );
        let err = settings.validate().expect_err("empty model should fail");
        assert!(err.contains("chat.model"));
    }


    #[test]
    fn invalid_base_url_is_invalid() {
        let mut settings = Settings::default();
        assert!(
            settings
                .set("chat.base_url", Value::String("not a url".into()))
                .is_ok(),
            "set should succeed"
        );
        let err = settings.validate().expect_err("bad URL should fail");
        assert!(err.contains("chat.base_url"));
    }


    #[test]
    fn target_exceeding_trigger_is_invalid() {
        let mut settings = Settings::default();
        assert!(
            settings
                .set("context.trigger_percentage", Value::Number(50.into()))
                .is_ok(),
            "set should succeed"
        );
        assert!(
            settings
                .set("context.target_percentage", Value::Number(75.into()))
                .is_ok(),
            "set should succeed"
        );
        let err = settings
            .validate()
            .expect_err("target > trigger should fail");
        assert!(err.contains("target_percentage"));
    }


    #[test]
    fn empty_memory_endpoint_url_is_invalid() {
        let mut settings = Settings::default();
        assert!(
            settings
                .set(
                    "memory.endpoints",
                    Value::Array(vec![serde_json::json!({
                        "id": "test",
                        "name": "Test",
                        "url": "",
                        "api_key": null,
                        "scope": null,
                    })]),
                )
                .is_ok(),
            "set should succeed"
        );
        let err = settings
            .validate()
            .expect_err("empty endpoint URL should fail");
        assert!(err.contains("memory.endpoints"));
    }
}
