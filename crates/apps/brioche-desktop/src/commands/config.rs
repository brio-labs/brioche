//! Application settings, profile management, and model configuration commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_shell_persistence::{Settings, profiles};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::{DesktopState, SessionMetadata};

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
    settings.save()?;

    // Update state factory settings in memory
    {
        let mut factory = state.factory.write().await;
        factory.settings = settings.clone();
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
        let handle = crate::commands::shell::build_shell(&current_id, &factory);

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
            manager
                .metadata_store
                .insert(SessionMetadata::new(&current_id, &workspace));
            let _ = manager.metadata_store.save();
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
/// Never panics. Returns Err on file write or profile creation failures.
#[tauri::command]
pub async fn create_profile(
    name: String,
    display_name: String,
    provider: String,
    model: String,
    api_key: String,
) -> Result<(), String> {
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
/// Never panics. Returns Err if profile update fails.
#[tauri::command]
pub async fn update_profile(profile: profiles::Profile) -> Result<(), String> {
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
