//! Profile management.
//!
//! Profiles allow users to have separate configurations, API keys, and
//! preferences. Each profile has its own directory under the config dir:
//! - Linux:   `~/.config/brioche-desktop/profiles/<name>/`
//! - macOS:   `~/Library/Application Support/brioche-desktop/profiles/<name>/`
//! - Windows: `%APPDATA%\brioche-desktop\profiles\<name>\`
//!
//! The active profile is stored in a global config file.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::path::PathBuf;

use brioche_shell_runtime::util::{load_json, save_json, system_time_secs};
use serde::{Deserialize, Serialize};

use crate::secrets::{protect_secret, reveal_secret};

/// A user profile.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    /// Unique name for this profile.
    pub name: String,
    /// Display name.
    pub display_name: String,
    /// Optional description.
    pub description: Option<String>,
    /// The LLM provider for this profile.
    pub provider: String,
    /// The model ID for this profile.
    pub model: String,
    /// API key, decrypted in memory and encrypted before disk persistence.
    pub api_key: String,
    /// Custom system prompt.
    pub system_prompt: Option<String>,
    /// Temperature setting (0.0 - 2.0).
    pub temperature: Option<f32>,
    /// Max tokens per response.
    pub max_tokens: Option<u32>,
    /// When this profile was created.
    pub created_at: u64,
    /// Whether this is the default profile.
    pub is_default: bool,
}

/// The global profile configuration.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// The currently active profile name.
    pub active: String,
    /// All available profiles.
    pub profiles: Vec<Profile>,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            active: "default".into(),
            profiles: vec![Profile {
                name: "default".into(),
                display_name: "Default".into(),
                description: Some("Default profile".into()),
                provider: "openrouter".into(),
                model: "qwen/qwen3.7-plus".into(),
                api_key: String::new(),
                system_prompt: None,
                temperature: Some(0.7),
                max_tokens: Some(4096),
                created_at: system_time_secs(),
                is_default: true,
            }],
        }
    }
}

impl ProfileConfig {
    /// Loads the profile config from disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn load() -> Self {
        let path = profile_config_path();
        match load_json::<_, ProfileConfig>(&path, "profiles") {
            Ok(mut config) => {
                config.reveal_api_keys();
                config
            }
            Err(_) => {
                let default = Self::default();
                let _ = default.save();
                default
            }
        }
    }

    /// Saves the profile config to disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn save(&self) -> Result<(), String> {
        let mut persisted = self.clone();
        persisted.protect_api_keys()?;
        save_json(profile_config_path(), &persisted, "profiles")
    }

    fn reveal_api_keys(&mut self) {
        for profile in &mut self.profiles {
            if let Ok(api_key) = reveal_secret(&profile.api_key) {
                profile.api_key = api_key;
            }
        }
    }

    fn protect_api_keys(&mut self) -> Result<(), String> {
        for profile in &mut self.profiles {
            profile.api_key = protect_secret(&profile.api_key)?;
        }
        Ok(())
    }

    /// Gets the active profile.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn active_profile(&self) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == self.active)
    }

    /// Gets a mutable reference to the active profile.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn active_profile_mut(&mut self) -> Option<&mut Profile> {
        let active_name = self.active.clone();
        self.profiles.iter_mut().find(|p| p.name == active_name)
    }

    /// Gets a profile by name.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// Creates a new profile.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn create(
        &mut self,
        name: String,
        display_name: String,
        provider: String,
        model: String,
        api_key: String,
    ) -> Result<(), String> {
        if self.profiles.iter().any(|p| p.name == name) {
            return Err(format!("Profile '{}' already exists", name));
        }
        self.profiles.push(Profile {
            name,
            display_name,
            description: None,
            provider,
            model,
            api_key,
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            created_at: system_time_secs(),
            is_default: false,
        });
        self.save()
    }

    /// Switches to a different profile.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn switch(&mut self, name: &str) -> Result<(), String> {
        if !self.profiles.iter().any(|p| p.name == name) {
            return Err(format!("Profile '{}' not found", name));
        }
        self.active = name.into();
        self.save()
    }

    /// Deletes a profile (cannot delete the default).
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn delete(&mut self, name: &str) -> Result<(), String> {
        if name == "default" {
            return Err("Cannot delete the default profile".into());
        }
        let len = self.profiles.len();
        self.profiles.retain(|p| p.name != name);
        if self.profiles.len() == len {
            return Err(format!("Profile '{}' not found", name));
        }
        if self.active == name {
            self.active = "default".into();
        }
        self.save()
    }

    /// Updates a profile.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn update(&mut self, profile: Profile) -> Result<(), String> {
        if let Some(idx) = self.profiles.iter().position(|p| p.name == profile.name) {
            self.profiles[idx] = profile;
            self.save()
        } else {
            Err(format!("Profile '{}' not found", profile.name))
        }
    }
}

fn profile_config_path() -> PathBuf {
    let config_dir = match dirs::config_dir() {
        Some(d) => d,
        None => std::env::temp_dir(),
    };
    config_dir.join("brioche-desktop").join("profiles.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_config_default() {
        let config = ProfileConfig::default();
        assert_eq!(config.active, "default");
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.profiles[0].name, "default");
    }

    #[test]
    fn profile_create_and_switch() {
        let mut config = ProfileConfig::default();
        assert!(
            config
                .create(
                    "work".into(),
                    "Work".into(),
                    "openrouter".into(),
                    "gpt-4".into(),
                    "key123".into(),
                )
                .is_ok(),
            "create profile should succeed"
        );
        assert_eq!(config.profiles.len(), 2);

        assert!(
            config.switch("work").is_ok(),
            "switch profile should succeed"
        );
        assert_eq!(config.active, "work");

        assert!(
            config.delete("work").is_ok(),
            "delete profile should succeed"
        );
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.active, "default");
    }

    #[test]
    fn profile_cannot_delete_default() {
        let mut config = ProfileConfig::default();
        assert!(config.delete("default").is_err());
    }

    #[test]
    fn profile_api_key_is_protected_for_persistence() -> Result<(), String> {
        let mut config = ProfileConfig::default();
        let profile = config
            .profiles
            .first_mut()
            .ok_or_else(|| "missing default profile".to_string())?;
        profile.api_key = "sk-profile-secret".into();
        config.protect_api_keys()?;
        let persisted = serde_json::to_string(&config).map_err(|err| err.to_string())?;

        assert!(persisted.contains("brioche-secret:v1:"));
        assert!(
            !persisted.contains("sk-profile-secret"),
            "serialized profile config must not contain plaintext API keys"
        );

        config.reveal_api_keys();
        assert_eq!(config.profiles[0].api_key, "sk-profile-secret");
        Ok(())
    }
}
