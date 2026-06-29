//! Application settings, profile management, and model configuration commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_shell_persistence::{ExtensionRegistry, Settings, profiles};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::{DesktopState, SessionMetadata};

/// Validates profile fields before creation or update.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1).
///
/// # Panic / Safety
/// Never panics.
fn validate_profile(profile: &profiles::Profile) -> Result<(), String> {
    if profile.name.trim().is_empty() {
        return Err("Profile name cannot be empty".into());
    }
    if profile.display_name.trim().is_empty() {
        return Err("Profile display name cannot be empty".into());
    }
    if profile.provider.trim().is_empty() {
        return Err("Provider cannot be empty".into());
    }
    if profile.model.trim().is_empty() {
        return Err("Model cannot be empty".into());
    }
    if let Some(temperature) = profile.temperature
        && !(0.0..=2.0).contains(&temperature)
    {
        return Err("Temperature must be between 0.0 and 2.0".into());
    }
    if let Some(max_tokens) = profile.max_tokens
        && max_tokens == 0
    {
        return Err("Max tokens must be greater than 0".into());
    }
    Ok(())
}

/// Returns the current user settings.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is configuration file size. Performs blocking disk read.
///
/// # Panic / Safety
/// Never panics. Returns default settings if loading fails.
#[tauri::command]
pub async fn get_settings() -> Result<Settings, String> {
    Ok(Settings::load())
}

/// Saves user settings.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(N) where N is serialized configuration size. Performs blocking disk write.
///
/// # Panic / Safety
/// Never panics. Returns Err on serialization or file write failure.
#[tauri::command]
pub async fn set_settings(
    app: AppHandle,
    state: State<'_, DesktopState>,
    settings: Settings,
) -> Result<(), String> {
    settings.validate()?;
    settings.save()?;

    // Rebuild the extension registry from the new settings so that AMP
    // endpoint changes take effect immediately.
    let extensions = ExtensionRegistry::default_set_from_settings(&settings);
    {
        let mut ext_guard = state.extensions.write().await;
        *ext_guard = extensions.clone();
    }

    // Update state factory settings and config in memory
    {
        let mut factory = state.factory.write().await;
        factory.config = crate::commands::shell::DesktopConfig::from_settings(&settings);
        factory.settings = settings.clone();
        factory.extensions = extensions;
    }

    // Rebuild the active session's shell if there is one
    let mut current_id_opt = None;
    {
        let mgr = state.manager.read().await;
        if let Some(manager) = mgr.as_ref() {
            current_id_opt = Some(manager.current_id().to_string());
        }
    }

    if let Some(current_id) = current_id_opt {
        let factory = state.factory.read().await.clone();
        let handle = crate::commands::shell::build_shell(&current_id, &factory).await;
        DesktopState::initialize_memory_providers(
            &factory,
            &current_id,
            &factory.settings.working_dir(),
        )?;
        let mut mgr = state.manager.write().await;
        if let Some(manager) = mgr.as_mut() {
            // Get all messages from the old shell/client
            let mut old_messages = Vec::new();
            if let Some(entry) = manager.sessions.get(&current_id) {
                old_messages = entry.history.read().await.clone();
            }

            // Push old messages to the new LLM history
            for msg in old_messages {
                handle.llm.push_message(msg).await;
            }

            // Insert new shell handle
            manager.insert(
                current_id.clone(),
                handle.shell,
                handle.llm,
                handle.history,
                handle.llm_rx,
            );

            // Update workspace metadata in store
            let workspace = factory.settings.working_dir();
            manager.insert_metadata(SessionMetadata::new(&current_id, &workspace))?;
        }

        // Notify the frontend of the update
        let _ = app.emit("session-changed", current_id.clone());
        let _ = app.emit("sessions-updated", ());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Profile commands
// ---------------------------------------------------------------------------

/// Profile payload for the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated profile settings. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
pub struct ProfilePayload {
    /// Machine-readable profile identifier (e.g., "default", "work").
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional description of the profile's purpose.
    pub description: Option<String>,
    /// LLM provider identifier (e.g., "openai", "openrouter").
    pub provider: String,
    /// Model identifier (e.g., "gpt-4o-mini").
    pub model: String,
    /// API key for the provider.
    pub api_key: String,
    /// Optional system prompt override.
    pub system_prompt: Option<String>,
    /// Optional temperature parameter (0.0–2.0).
    pub temperature: Option<f32>,
    /// Optional max tokens limit.
    pub max_tokens: Option<u32>,
    /// Unix timestamp when the profile was created.
    pub created_at: u64,
    /// Whether this is the default active profile.
    pub is_default: bool,
}

impl From<&profiles::Profile> for ProfilePayload {
    fn from(p: &profiles::Profile) -> Self {
        Self {
            name: p.name.clone(),
            display_name: p.display_name.clone(),
            description: p.description.clone(),
            provider: p.provider.clone(),
            model: p.model.clone(),
            api_key: p.api_key.clone(),
            system_prompt: p.system_prompt.clone(),
            temperature: p.temperature,
            max_tokens: p.max_tokens,
            created_at: p.created_at,
            is_default: p.is_default,
        }
    }
}

/// Lists all profiles.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of defined profiles. Reads profile config from disk.
///
/// # Panic / Safety
/// Never panics. Returns empty list if load fails.
#[tauri::command]
pub async fn list_profiles() -> Result<Vec<ProfilePayload>, String> {
    let config = profiles::ProfileConfig::load();
    Ok(config.profiles.iter().map(ProfilePayload::from).collect())
}

/// Gets the active profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of profiles. Reads profile config from disk.
///
/// # Panic / Safety
/// Never panics. Returns None if name or active profile not found.
#[tauri::command]
pub async fn get_profile(name: Option<String>) -> Result<Option<ProfilePayload>, String> {
    let config = profiles::ProfileConfig::load();
    if let Some(n) = name {
        Ok(config.get(&n).map(ProfilePayload::from))
    } else {
        Ok(config.active_profile().map(ProfilePayload::from))
    }
}

/// Creates a new profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of profiles. Reads and writes profile config disk.
///
/// # Panic / Safety
/// Never panics. Returns Err on invalid input, file write, or profile creation failures.
#[tauri::command]
pub async fn create_profile(
    name: String,
    display_name: String,
    provider: String,
    model: String,
    api_key: String,
) -> Result<(), String> {
    let candidate = profiles::Profile {
        name: name.clone(),
        display_name: display_name.clone(),
        description: None,
        provider: provider.clone(),
        model: model.clone(),
        api_key: api_key.clone(),
        system_prompt: None,
        temperature: Some(0.7),
        max_tokens: Some(4096),
        created_at: 0,
        is_default: false,
    };
    validate_profile(&candidate)?;

    let mut config = profiles::ProfileConfig::load();
    config.create(name, display_name, provider, model, api_key)
}

/// Switches to a different profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of profiles. Reads and writes profile config disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if switch fails.
#[tauri::command]
pub async fn switch_profile(name: String) -> Result<(), String> {
    let mut config = profiles::ProfileConfig::load();
    config.switch(&name)
}

/// Deletes a profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of profiles. Reads and writes profile config disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if deletion fails.
#[tauri::command]
pub async fn delete_profile(name: String) -> Result<(), String> {
    let mut config = profiles::ProfileConfig::load();
    config.delete(&name)
}

/// Updates a profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(P) where P is the number of profiles. Reads and writes profile config disk.
///
/// # Panic / Safety
/// Never panics. Returns Err if validation or profile update fails.
#[tauri::command]
pub async fn update_profile(profile: profiles::Profile) -> Result<(), String> {
    validate_profile(&profile)?;
    let mut config = profiles::ProfileConfig::load();
    config.update(profile)
}

// ---------------------------------------------------------------------------
// Model fetching
// ---------------------------------------------------------------------------

/// Available model info.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated model info. O(1).
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "openai/gpt-4o").
    pub id: String,
    /// Human-readable model name.
    pub name: String,
    /// Provider identifier (e.g., "openai", "anthropic").
    pub provider: String,
}

/// OpenRouter API response structure.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

/// OpenRouter model entry.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    name: Option<String>,
}

/// Fetches available models from OpenRouter.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Performs async HTTP request over the network. O(M) where M is response size.
///
/// # Panic / Safety
/// Never panics. Returns Err if HTTP request or JSON parsing fails.
#[tauri::command]
pub async fn fetch_models() -> Result<Vec<ModelInfo>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://openrouter.ai/api/v1/models")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("OpenRouter returned status: {}", resp.status()));
    }

    let body: OpenRouterModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse models response: {e}"))?;

    let models = body
        .data
        .into_iter()
        .map(|item| ModelInfo {
            id: item.id.clone(),
            name: match item.name {
                Some(n) => n,
                None => item.id.clone(),
            },
            provider: "openrouter".into(),
        })
        .collect();

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> profiles::Profile {
        profiles::Profile {
            name: "work".into(),
            display_name: "Work".into(),
            description: Some("work profile".into()),
            provider: "openrouter".into(),
            model: "qwen/qwen3.7-plus".into(),
            api_key: "key".into(),
            system_prompt: Some("be helpful".into()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            created_at: 1234567890,
            is_default: false,
        }
    }

    #[test]
    fn profile_payload_from_profile() -> Result<(), String> {
        let profile = sample_profile();
        let payload = ProfilePayload::from(&profile);
        assert_eq!(payload.name, "work");
        assert_eq!(payload.display_name, "Work");
        assert_eq!(payload.description.as_deref(), Some("work profile"));
        assert_eq!(payload.provider, "openrouter");
        assert_eq!(payload.model, "qwen/qwen3.7-plus");
        assert_eq!(payload.api_key, "key");
        assert_eq!(payload.temperature, Some(0.7));
        assert_eq!(payload.max_tokens, Some(4096));
        assert_eq!(payload.created_at, 1234567890);
        assert!(!payload.is_default);
        Ok(())
    }

    #[test]
    fn validate_profile_accepts_valid_profile() -> Result<(), String> {
        validate_profile(&sample_profile())?;
        Ok(())
    }

    #[test]
    fn validate_profile_rejects_empty_name() -> Result<(), String> {
        let mut profile = sample_profile();
        profile.name = "   ".into();
        match validate_profile(&profile) {
            Err(e) => assert_eq!(e, "Profile name cannot be empty"),
            Ok(_) => return Err("expected empty name to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn validate_profile_rejects_empty_provider() -> Result<(), String> {
        let mut profile = sample_profile();
        profile.provider = "".into();
        match validate_profile(&profile) {
            Err(e) => assert_eq!(e, "Provider cannot be empty"),
            Ok(_) => return Err("expected empty provider to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn validate_profile_rejects_empty_model() -> Result<(), String> {
        let mut profile = sample_profile();
        profile.model = "".into();
        match validate_profile(&profile) {
            Err(e) => assert_eq!(e, "Model cannot be empty"),
            Ok(_) => return Err("expected empty model to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn validate_profile_rejects_out_of_range_temperature() -> Result<(), String> {
        let mut profile = sample_profile();
        profile.temperature = Some(2.5);
        match validate_profile(&profile) {
            Err(e) => assert_eq!(e, "Temperature must be between 0.0 and 2.0"),
            Ok(_) => return Err("expected out-of-range temperature to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn validate_profile_rejects_zero_max_tokens() -> Result<(), String> {
        let mut profile = sample_profile();
        profile.max_tokens = Some(0);
        match validate_profile(&profile) {
            Err(e) => assert_eq!(e, "Max tokens must be greater than 0"),
            Ok(_) => return Err("expected zero max tokens to be rejected".into()),
        }
        Ok(())
    }

    #[test]
    fn model_info_falls_back_to_id_when_name_missing() -> Result<(), String> {
        let model = OpenRouterModel {
            id: "openai/gpt-4o".into(),
            name: None,
        };
        let info = ModelInfo {
            id: model.id.clone(),
            name: match model.name {
                Some(n) => n,
                None => model.id.clone(),
            },
            provider: "openrouter".into(),
        };
        assert_eq!(info.id, "openai/gpt-4o");
        assert_eq!(info.name, "openai/gpt-4o");
        assert_eq!(info.provider, "openrouter");
        Ok(())
    }
}
