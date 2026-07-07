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
use std::sync::OnceLock;

use brioche_shell_runtime::util::{load_json, save_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::secrets::{protect_secret, reveal_secret};

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

impl Default for Settings {
    fn default() -> Self {
        Self {
            modules: schema_default_modules().clone(),
        }
    }
}

fn schema_default_modules() -> &'static BTreeMap<String, Value> {
    static DEFAULTS: OnceLock<BTreeMap<String, Value>> = OnceLock::new();

    DEFAULTS.get_or_init(|| {
        let mut settings = Settings {
            modules: BTreeMap::new(),
        };

        for section in crate::extensions::settings_sections::all_builtin_sections() {
            for field in section.fields {
                if let Some(value) = field.default_value {
                    let _ = settings.set(&field.key, value);
                }
            }
        }
        settings.modules
    })
}

fn get_from_modules(modules: &BTreeMap<String, Value>, key: &str) -> Option<Value> {
    let mut parts = key.split('.');
    let module = parts.next()?;
    let mut value = modules.get(module)?;
    for part in parts {
        value = value.get(part)?;
    }
    Some(value.clone())
}
#[inline]
#[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
fn option_or<T>(value: Option<T>, default: T) -> T {
    match value {
        Some(value) => value,
        None => default,
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
        match load_json::<_, Settings>(&path, "settings") {
            Ok(mut settings) => {
                // Merge missing default module values so upgrades keep working.
                let defaults = Self::default();
                for (key, value) in defaults.modules {
                    settings.modules.entry(key).or_insert(value);
                }
                settings.reveal_api_keys();
                settings
            }
            Err(_) => Self::default(),
        }
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
        let mut persisted = self.clone();
        persisted.protect_api_keys()?;
        save_json(settings_path(), &persisted, "settings")
    }

    fn reveal_api_keys(&mut self) {
        reveal_secret_at(&mut self.modules, &["chat", "api_key"]);
        reveal_secret_array_field(&mut self.modules, &["chat", "fallback_models"], "api_key");
        reveal_secret_array_field(&mut self.modules, &["memory", "endpoints"], "api_key");
    }

    fn protect_api_keys(&mut self) -> Result<(), String> {
        protect_secret_at(&mut self.modules, &["chat", "api_key"])?;
        protect_secret_array_field(&mut self.modules, &["chat", "fallback_models"], "api_key")?;
        protect_secret_array_field(&mut self.modules, &["memory", "endpoints"], "api_key")?;
        Ok(())
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
        get_from_modules(&self.modules, key)
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

    /// Returns a dotted value, falling back to the schema-derived default.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn get_with_schema_default(&self, key: &str) -> Option<Value> {
        self.get(key)
            .or_else(|| get_from_modules(schema_default_modules(), key))
    }

    /// Returns the working directory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn working_dir(&self) -> String {
        option_or(
            self.get_with_schema_default("ui.working_dir")
                .and_then(|v| v.as_str().map(ToString::to_string)),
            "workspace".into(),
        )
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
        option_or(
            self.get_with_schema_default("chat.provider")
                .and_then(|v| v.as_str().map(ToString::to_string)),
            "openrouter".into(),
        )
    }

    /// Returns the active chat model.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn chat_model(&self) -> String {
        option_or(
            self.get_with_schema_default("chat.model")
                .and_then(|v| v.as_str().map(ToString::to_string)),
            "qwen/qwen3.7-plus".into(),
        )
    }

    /// Returns the configured API key.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn api_key(&self) -> String {
        self.get_with_schema_default("chat.api_key")
            .and_then(|v| v.as_str().map(ToString::to_string))
            .map_or(String::new(), |secret| {
                reveal_secret(&secret).map_or(String::new(), |secret| secret)
            })
    }

    /// Returns the configured base URL.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn base_url(&self) -> String {
        option_or(
            self.get_with_schema_default("chat.base_url")
                .and_then(|v| v.as_str().map(ToString::to_string)),
            "https://openrouter.ai/api/v1".into(),
        )
    }

    /// Returns the configured max tokens.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn max_tokens(&self) -> u32 {
        option_or(
            self.get_with_schema_default("chat.max_tokens")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok()),
            4096,
        )
    }

    /// Returns the configured context window.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn context_window(&self) -> usize {
        option_or(
            self.get_with_schema_default("chat.context_window")
                .and_then(|v| v.as_u64())
                .and_then(|v| usize::try_from(v).ok()),
            128_000,
        )
    }

    /// Returns whether streaming is enabled.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn stream(&self) -> bool {
        option_or(
            self.get_with_schema_default("ui.stream")
                .and_then(|v| v.as_bool()),
            true,
        )
    }

    /// Returns whether context compression is enabled.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn context_enabled(&self) -> bool {
        option_or(
            self.get_with_schema_default("context.enabled")
                .and_then(|v| v.as_bool()),
            true,
        )
    }

    /// Returns the trigger percentage that starts context compression.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn context_trigger_percentage(&self) -> u8 {
        option_or(
            self.get_with_schema_default("context.trigger_percentage")
                .and_then(|v| v.as_u64())
                .and_then(|v| u8::try_from(v).ok()),
            75,
        )
    }

    /// Returns the target percentage to compress context to.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn context_target_percentage(&self) -> u8 {
        option_or(
            self.get_with_schema_default("context.target_percentage")
                .and_then(|v| v.as_u64())
                .and_then(|v| u8::try_from(v).ok()),
            50,
        )
    }

    /// Returns the number of recent messages to always preserve.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn context_preserve_recent(&self) -> usize {
        option_or(
            self.get_with_schema_default("context.preserve_recent")
                .and_then(|v| v.as_u64())
                .and_then(|v| usize::try_from(v).ok()),
            6,
        )
    }

    /// Returns whether reasoning is enabled for supported chat models.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn reasoning_enabled(&self) -> bool {
        option_or(
            self.get_with_schema_default("chat.reasoning_enabled")
                .and_then(|v| v.as_bool()),
            false,
        )
    }

    /// Returns the configured reasoning effort.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn reasoning_effort(&self) -> String {
        option_or(
            self.get_with_schema_default("chat.reasoning_effort")
                .and_then(|v| v.as_str().map(ToString::to_string)),
            "medium".into(),
        )
    }

    /// Returns the active memory provider ids.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn active_memory_providers(&self) -> Vec<String> {
        let providers = option_or(
            self.get_with_schema_default("memory.active_providers")
                .and_then(|v| v.as_array().cloned()),
            vec![serde_json::Value::String("memory-local".into())],
        );
        providers
            .iter()
            .filter_map(|v| v.as_str().map(ToString::to_string))
            .collect()
    }

    /// Returns configured AMP-compatible memory endpoints.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn memory_endpoints(&self) -> Vec<MemoryEndpoint> {
        let endpoints = option_or(
            self.get_with_schema_default("memory.endpoints")
                .and_then(|v| v.as_array().cloned()),
            Vec::new(),
        );
        endpoints
            .into_iter()
            .filter_map(|v| serde_json::from_value::<MemoryEndpoint>(v).ok())
            .map(|mut endpoint| {
                if let Some(api_key) = endpoint.api_key.take() {
                    endpoint.api_key = reveal_secret(&api_key).ok();
                }
                endpoint
            })
            .collect()
    }

    /// Returns fallback models.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn fallback_models(&self) -> Vec<FallbackModel> {
        let models = option_or(
            self.get_with_schema_default("chat.fallback_models")
                .and_then(|v| serde_json::from_value::<Vec<FallbackModel>>(v).ok()),
            Vec::new(),
        );
        models
            .into_iter()
            .map(|mut model| {
                if let Some(api_key) = model.api_key.take() {
                    model.api_key = reveal_secret(&api_key).ok();
                }
                model
            })
            .collect()
    }

    /// Returns the current system prompt.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn system_prompt(&self) -> String {
        option_or(
            self.get_with_schema_default("chat.system_prompt")
                .and_then(|v| v.as_str().map(ToString::to_string)),
            "You are a helpful AI coding assistant with access to filesystem tools.".into(),
        )
    }

    /// Returns the current personality.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn personality(&self) -> String {
        option_or(
            self.get_with_schema_default("chat.personality")
                .and_then(|v| v.as_str().map(ToString::to_string)),
            "helpful".into(),
        )
    }

    /// User-defined tools execute arbitrary shell commands or HTTP requests and
    /// are disabled for safety.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn user_tools_enabled(&self) -> bool {
        option_or(
            self.get_with_schema_default("tools.user_tools_enabled")
                .and_then(|v| v.as_bool()),
            false,
        )
    }

    /// Returns additional command names the user has allowed for the built-in
    /// `execute_command` tool.
    ///
    /// These names extend the desktop application's default allow-list.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn allowed_commands(&self) -> Vec<String> {
        let commands = option_or(
            self.get_with_schema_default("tools.allowed_commands")
                .and_then(|v| v.as_array().cloned()),
            Vec::new(),
        );
        commands
            .iter()
            .filter_map(|item| item.as_str().map(ToString::to_string))
            .collect()
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
            .get_with_schema_default("context.trigger_percentage")
            .and_then(|v| v.as_u64());
        let target = self
            .get_with_schema_default("context.target_percentage")
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
}

fn reveal_secret_at(modules: &mut BTreeMap<String, Value>, path: &[&str]) {
    if let Some(Value::String(secret)) = value_at_mut(modules, path)
        && let Ok(revealed) = reveal_secret(secret)
    {
        *secret = revealed;
    }
}

fn protect_secret_at(modules: &mut BTreeMap<String, Value>, path: &[&str]) -> Result<(), String> {
    if let Some(Value::String(secret)) = value_at_mut(modules, path) {
        *secret = protect_secret(secret)?;
    }
    Ok(())
}

fn reveal_secret_array_field(modules: &mut BTreeMap<String, Value>, path: &[&str], field: &str) {
    if let Some(Value::Array(items)) = value_at_mut(modules, path) {
        for item in items {
            if let Value::Object(object) = item
                && let Some(Value::String(secret)) = object.get_mut(field)
                && let Ok(revealed) = reveal_secret(secret)
            {
                *secret = revealed;
            }
        }
    }
}

fn protect_secret_array_field(
    modules: &mut BTreeMap<String, Value>,
    path: &[&str],
    field: &str,
) -> Result<(), String> {
    if let Some(Value::Array(items)) = value_at_mut(modules, path) {
        for item in items {
            if let Value::Object(object) = item
                && let Some(Value::String(secret)) = object.get_mut(field)
            {
                *secret = protect_secret(secret)?;
            }
        }
    }
    Ok(())
}

fn value_at_mut<'a>(
    modules: &'a mut BTreeMap<String, Value>,
    path: &[&str],
) -> Option<&'a mut Value> {
    let (module, fields) = path.split_first()?;
    let mut current = modules.get_mut(*module)?;
    for field in fields {
        current = current.get_mut(*field)?;
    }
    Some(current)
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
    use crate::extensions::settings_sections::SettingsListRenderer;
    #[allow(clippy::panic)]
    #[test]
    fn section_defaults_match_schema_defaults() {
        let defaults = Settings::default();
        let registry = crate::extensions::ExtensionRegistry::default_set_from_settings(&defaults);
        let sections: Vec<_> = registry
            .settings_sections()
            .iter()
            .flat_map(|provider| provider.sections())
            .collect();

        let find_field = |key: &str| {
            sections
                .iter()
                .flat_map(|section| section.fields.iter())
                .find(|field| field.key == key)
        };

        let assert_default_value = |key: &str| {
            let Some(expected) = defaults.get(key) else {
                panic!("default key missing: {key}");
            };
            let Some(actual) = find_field(key).and_then(|field| field.default_value.clone()) else {
                panic!("settings schema missing default for {key}");
            };
            assert_eq!(
                actual, expected,
                "schema default for `{key}` should match settings defaults"
            );
        };

        assert_default_value("chat.model");
        assert_default_value("chat.base_url");
        assert_default_value("context.enabled");
        assert_default_value("context.trigger_percentage");
        assert_default_value("context.target_percentage");
        assert_default_value("context.preserve_recent");
        assert_default_value("chat.reasoning_enabled");
        assert_default_value("chat.reasoning_effort");
        assert_default_value("memory.endpoints");
        assert_default_value("tools.allowed_commands");
        assert_default_value("ui.stream");
        assert_default_value("ui.working_dir");

        let Some(fallback_schema) =
            find_field("chat.fallback_models").and_then(|field| field.list_schema.as_ref())
        else {
            panic!("fallback schema missing");
        };
        assert_eq!(fallback_schema.renderer, SettingsListRenderer::Record);

        let Some(memory_schema) =
            find_field("memory.endpoints").and_then(|field| field.list_schema.as_ref())
        else {
            panic!("memory schema missing");
        };
        assert_eq!(
            memory_schema.renderer,
            SettingsListRenderer::MemoryEndpoints
        );
        assert_eq!(
            memory_schema.item_schema.len(),
            5,
            "memory schema should expose all endpoint fields"
        );

        let Some(allowed_command_schema) =
            find_field("tools.allowed_commands").and_then(|field| field.list_schema.as_ref())
        else {
            panic!("allowed commands schema missing");
        };
        assert_eq!(
            allowed_command_schema.renderer,
            SettingsListRenderer::String
        );
    }

    #[test]
    fn list_renderer_serializes_ipc_snake_case() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_value(SettingsListRenderer::Record)?,
            Value::String("record".into())
        );
        assert_eq!(
            serde_json::to_value(SettingsListRenderer::MemoryEndpoints)?,
            Value::String("memory_endpoints".into())
        );
        assert_eq!(
            serde_json::to_value(SettingsListRenderer::String)?,
            Value::String("string".into())
        );
        Ok(())
    }

    #[test]
    fn partial_modules_use_schema_defaults_for_missing_fields() -> Result<(), String> {
        let defaults = Settings::default();
        let expected_working_dir = defaults
            .get("ui.working_dir")
            .and_then(|value| value.as_str().map(ToString::to_string))
            .ok_or_else(|| "default ui.working_dir missing".to_string())?;

        let mut modules = BTreeMap::new();
        modules.insert(
            "ui".into(),
            Value::Object(
                [("stream".into(), Value::Bool(false))]
                    .into_iter()
                    .collect(),
            ),
        );
        modules.insert(
            "context".into(),
            Value::Object(
                [("enabled".into(), Value::Bool(false))]
                    .into_iter()
                    .collect(),
            ),
        );
        modules.insert(
            "chat".into(),
            Value::Object(
                [("provider".into(), Value::String("anthropic".into()))]
                    .into_iter()
                    .collect(),
            ),
        );
        let settings = Settings { modules };

        assert_eq!(settings.working_dir(), expected_working_dir);
        assert!(!settings.context_enabled());
        assert_eq!(settings.context_trigger_percentage(), 75);
        assert_eq!(settings.context_target_percentage(), 50);
        assert_eq!(settings.context_preserve_recent(), 6);
        assert_eq!(settings.chat_provider(), "anthropic");
        assert_eq!(settings.chat_model(), "qwen/qwen3.7-plus");
        let endpoint_ids: Vec<_> = settings
            .memory_endpoints()
            .into_iter()
            .map(|endpoint| endpoint.id)
            .collect();
        let default_endpoint_ids: Vec<_> = defaults
            .memory_endpoints()
            .into_iter()
            .map(|endpoint| endpoint.id)
            .collect();
        assert_eq!(endpoint_ids, default_endpoint_ids);
        Ok(())
    }

    #[test]
    fn context_and_reasoning_accessors_default_from_schema() {
        let mut settings = Settings::default();

        assert!(settings.context_enabled());
        assert_eq!(settings.context_trigger_percentage(), 75);
        assert_eq!(settings.context_target_percentage(), 50);
        assert_eq!(settings.context_preserve_recent(), 6);
        assert!(!settings.reasoning_enabled());
        assert_eq!(settings.reasoning_effort(), "medium");

        assert!(
            settings
                .set("context.trigger_percentage", Value::Number(82.into()))
                .is_ok(),
            "setting context.trigger_percentage should succeed"
        );
        assert_eq!(settings.context_trigger_percentage(), 82);
        assert!(
            settings
                .set("context.preserve_recent", Value::Number(12.into()))
                .is_ok(),
            "setting context.preserve_recent should succeed"
        );
        assert_eq!(settings.context_preserve_recent(), 12);

        assert!(
            settings
                .set("context.trigger_percentage", Value::String("bad".into()))
                .is_ok(),
            "setting invalid trigger should succeed"
        );
        assert_eq!(
            settings.context_trigger_percentage(),
            75,
            "invalid numeric values should fall back to schema defaults"
        );

        assert!(
            settings
                .set("chat.reasoning_enabled", Value::Bool(true))
                .is_ok(),
            "setting reasoning_enabled should succeed"
        );
        assert!(settings.reasoning_enabled());
        assert!(
            settings
                .set("chat.reasoning_effort", Value::String("high".into()))
                .is_ok(),
            "setting reasoning_effort should succeed"
        );
        assert_eq!(settings.reasoning_effort(), "high");
    }
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
            settings.set("chat.model", Value::String("".into())).is_ok(),
            "set should succeed"
        );
        assert!(
            settings
                .validate()
                .is_err_and(|err| err.contains("chat.model")),
            "empty model should fail"
        );
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
        assert!(
            settings
                .validate()
                .is_err_and(|err| err.contains("chat.base_url")),
            "bad URL should fail"
        );
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
        assert!(
            settings
                .validate()
                .is_err_and(|err| err.contains("target_percentage")),
            "target > trigger should fail"
        );
    }

    #[test]
    fn settings_api_keys_are_protected_for_persistence() -> Result<(), String> {
        let mut settings = Settings::default();
        settings.set("chat.api_key", Value::String("sk-chat-secret".into()))?;
        settings.set(
            "chat.fallback_models",
            Value::Array(vec![Value::Object(
                [
                    ("provider".into(), Value::String("openai".into())),
                    ("model".into(), Value::String("gpt-4o".into())),
                    ("api_key".into(), Value::String("sk-fallback-secret".into())),
                ]
                .into_iter()
                .collect(),
            )]),
        )?;

        settings.protect_api_keys()?;
        let persisted = serde_json::to_string(&settings).map_err(|err| err.to_string())?;

        assert!(persisted.contains("brioche-secret:v1:"));
        assert!(!persisted.contains("sk-chat-secret"));
        assert!(!persisted.contains("sk-fallback-secret"));
        settings.reveal_api_keys();
        assert_eq!(settings.api_key(), "sk-chat-secret");
        let fallback = settings
            .fallback_models()
            .into_iter()
            .next()
            .ok_or_else(|| "missing fallback model".to_string())?;
        assert_eq!(fallback.api_key.as_deref(), Some("sk-fallback-secret"));
        Ok(())
    }

    #[test]
    fn empty_memory_endpoint_url_is_invalid() {
        let mut settings = Settings::default();
        let mut endpoint = serde_json::Map::new();
        endpoint.insert("id".into(), Value::String("test".into()));
        endpoint.insert("name".into(), Value::String("Test".into()));
        endpoint.insert("url".into(), Value::String("".into()));
        endpoint.insert("api_key".into(), Value::Null);
        endpoint.insert("scope".into(), Value::Null);
        assert!(
            settings
                .set(
                    "memory.endpoints",
                    Value::Array(vec![Value::Object(endpoint)])
                )
                .is_ok(),
            "set should succeed"
        );
        assert!(
            settings
                .validate()
                .is_err_and(|err| err.contains("memory.endpoints")),
            "empty endpoint URL should fail"
        );
    }
}
