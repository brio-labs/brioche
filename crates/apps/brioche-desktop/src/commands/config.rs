//! Application settings, profile management, and model configuration commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_shell_persistence::{ExtensionRegistry, Settings};
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
        let handle = crate::commands::shell::build_shell(&current_id, &factory)
            .await
            .map_err(|e| e.to_string())?;
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

// Profile commands removed as Settings is now the single source of truth

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
