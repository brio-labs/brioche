//! Modular skill provider extension point.
//!
//! Skills are prompt packages that can be enabled, disabled, tagged and sorted.
//! The default provider scans the Hermes skills directory; future providers can
//! load skills from a workspace, a git repository or a remote registry.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::RwLock;

use serde::{Deserialize, Serialize};

use super::{ExtensionMetadata, PanelSlot};
/// A skill descriptor.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillDescriptor {
    /// Skill name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Version string.
    pub version: String,
    /// Author.
    pub author: String,
    /// License.
    pub license: String,
    /// Supported platforms.
    pub platforms: Vec<String>,
    /// Category / folder.
    pub category: String,
    /// Absolute filesystem path.
    pub path: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Related skill names.
    pub related_skills: Vec<String>,
    /// Markdown content without frontmatter.
    pub content: String,
    /// Whether the skill is currently enabled.
    pub enabled: bool,
}

/// Extension trait for skill providers.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub trait SkillProvider: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;

    /// Scans and returns all available skills.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn list(&self) -> Result<Vec<SkillDescriptor>, String>;

    /// Reads the raw markdown content of a skill.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn read_content(&self, name: &str) -> Result<String, String>;

    /// Reads an auxiliary file from a skill directory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn read_file(&self, name: &str, file_path: &str) -> Result<String, String>;

    /// Enables or disables a skill.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), String>;

    /// Creates a new skill package.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn create_skill(
        &self,
        name: &str,
        category: &str,
        description: &str,
        content: &str,
    ) -> Result<(), String>;

    /// Deletes a skill package.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn delete_skill(&self, name: &str) -> Result<(), String>;
}

/// Default skill registry that scans `~/.hermes/skills/`.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Debug)]
pub struct SkillRegistry {
    enabled: RwLock<Vec<String>>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        let enabled = match Self::load_enabled() {
            Ok(enabled) => enabled,
            Err(err) => {
                tracing::warn!("Failed to load enabled skills, using defaults: {err}");
                Vec::new()
            }
        };
        Self {
            enabled: RwLock::new(enabled),
        }
    }
}

impl SkillRegistry {
    /// Returns the Hermes skills directory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn skills_dir() -> std::path::PathBuf {
        let home = match dirs::home_dir() {
            Some(d) => d,
            None => std::env::temp_dir(),
        };
        home.join(".hermes").join("skills")
    }

    /// Loads enabled state from disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the size of the enabled-skills JSON file. Performs blocking file I/O.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if the file cannot be read or parsed.
    fn load_enabled() -> Result<Vec<String>, String> {
        let path = Self::enabled_path();
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read enabled skills: {e}"))?;
        serde_json::from_str::<Vec<String>>(&data)
            .map_err(|e| format!("Failed to parse enabled skills: {e}"))
    }

    /// Saves enabled state to disk.
    fn save_enabled(&self) -> Result<(), String> {
        let path = Self::enabled_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create skills dir: {e}"))?;
        }
        let enabled = self
            .enabled
            .read()
            .map_err(|_| "Skill registry lock poisoned".to_string())?;
        let data = serde_json::to_string_pretty(&*enabled)
            .map_err(|e| format!("Failed to serialize enabled skills: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write enabled skills: {e}"))
    }

    fn enabled_path() -> std::path::PathBuf {
        let config_dir = match dirs::config_dir() {
            Some(d) => d,
            None => std::env::temp_dir(),
        };
        config_dir
            .join("brioche-desktop")
            .join("skills-enabled.json")
    }
}

impl SkillProvider for SkillRegistry {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "skills-local".into(),
            name: "Local skill scanner".into(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Left),
            enabled: true,
        }
    }

    fn list(&self) -> Result<Vec<SkillDescriptor>, String> {
        let skills_dir = Self::skills_dir();
        if !skills_dir.exists() {
            return Ok(Vec::new());
        }
        let mut skills = Vec::new();
        scan_dir(&skills_dir, &skills_dir, &mut skills);
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        let enabled = self
            .enabled
            .read()
            .map_err(|_| "Skill registry lock poisoned".to_string())?;
        let all_enabled = enabled.is_empty();
        Ok(skills
            .into_iter()
            .map(|mut s| {
                s.enabled = all_enabled || enabled.contains(&s.name);
                s
            })
            .collect())
    }

    fn read_content(&self, name: &str) -> Result<String, String> {
        let skills = self.list()?;
        let skill = skills
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("Skill '{name}' not found"))?;
        let skill_md = std::path::PathBuf::from(&skill.path).join("SKILL.md");
        std::fs::read_to_string(&skill_md).map_err(|e| format!("Failed to read skill content: {e}"))
    }

    fn read_file(&self, name: &str, file_path: &str) -> Result<String, String> {
        let skills = self.list()?;
        let skill = skills
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("Skill '{name}' not found"))?;
        let full_path = std::path::PathBuf::from(&skill.path).join(file_path);
        std::fs::read_to_string(&full_path).map_err(|e| format!("Failed to read skill file: {e}"))
    }

    fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), String> {
        let mut guard = self
            .enabled
            .write()
            .map_err(|_| "Skill registry lock poisoned".to_string())?;
        if enabled {
            let key = name.to_string();
            if !guard.contains(&key) {
                guard.push(key);
            }
        } else {
            guard.retain(|n| n != name);
        }
        drop(guard);
        self.save_enabled()
    }

    fn create_skill(
        &self,
        name: &str,
        category: &str,
        description: &str,
        content: &str,
    ) -> Result<(), String> {
        if name.is_empty() {
            return Err("Skill name cannot be empty".into());
        }
        let skills_dir = Self::skills_dir();
        let dir = skills_dir.join(category).join(name);
        if dir.exists() {
            return Err(format!("Skill '{}' already exists", name));
        }
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create skill directory: {e}"))?;

        let frontmatter = format!(
            "---\nname: {}\ndescription: {}\nversion: 0.1.0\nauthor: user\nlicense: MIT\ntags: []\n---\n\n",
            name, description
        );
        let skill_md = dir.join("SKILL.md");
        std::fs::write(&skill_md, format!("{}{}", frontmatter, content))
            .map_err(|e| format!("Failed to write SKILL.md: {e}"))?;

        // Enable the new skill by default.
        let mut guard = self
            .enabled
            .write()
            .map_err(|_| "Skill registry lock poisoned".to_string())?;
        let key = name.to_string();
        if !guard.contains(&key) {
            guard.push(key);
        }
        drop(guard);
        self.save_enabled()
    }

    fn delete_skill(&self, name: &str) -> Result<(), String> {
        let skills = self.list()?;
        let skill = skills
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("Skill '{}' not found", name))?;
        let path = std::path::PathBuf::from(&skill.path);
        std::fs::remove_dir_all(&path)
            .map_err(|e| format!("Failed to delete skill directory: {e}"))?;
        let mut guard = self
            .enabled
            .write()
            .map_err(|_| "Skill registry lock poisoned".to_string())?;
        guard.retain(|n| n != name);
        drop(guard);
        self.save_enabled()
    }
}

fn scan_dir(root: &std::path::Path, current: &std::path::Path, out: &mut Vec<SkillDescriptor>) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Some(skill) = parse_skill(&skill_md, root) {
                    out.push(skill);
                }
            } else {
                scan_dir(root, &path, out);
            }
        }
    }
}

fn parse_skill(skill_md: &std::path::Path, root: &std::path::Path) -> Option<SkillDescriptor> {
    let content = std::fs::read_to_string(skill_md).ok()?;
    let (frontmatter, body) = split_frontmatter(&content)?;

    let fields: std::collections::BTreeMap<&str, &str> = frontmatter
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once(':')
                .map(|(k, v)| (k.trim(), v.trim().trim_matches('"').trim_matches('\'')))
        })
        .collect();

    fn get(fields: &std::collections::BTreeMap<&str, &str>, key: &str) -> String {
        match fields.get(key) {
            Some(v) => (*v).to_string(),
            None => String::new(),
        }
    }

    fn list_field(fields: &std::collections::BTreeMap<&str, &str>, key: &str) -> Vec<String> {
        let raw = match fields.get(key) {
            Some(v) => *v,
            None => "[]",
        };
        raw.trim_matches('[')
            .trim_matches(']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    let name = {
        let n = get(&fields, "name");
        if n.is_empty() {
            match skill_md.parent().and_then(|p| p.file_name()) {
                Some(n) => n.to_string_lossy().to_string(),
                None => String::new(),
            }
        } else {
            n
        }
    };

    let category = match skill_md
        .parent()
        .and_then(|p| p.parent())
        .and_then(|parent| parent.strip_prefix(root).ok())
        .and_then(|rel| rel.components().next())
    {
        Some(c) => c.as_os_str().to_string_lossy().to_string(),
        None => "uncategorized".into(),
    };

    let path_str = match skill_md.parent() {
        Some(p) => p.to_string_lossy().to_string(),
        None => String::new(),
    };
    Some(SkillDescriptor {
        name,
        description: get(&fields, "description"),
        version: get(&fields, "version"),
        author: get(&fields, "author"),
        license: get(&fields, "license"),
        platforms: list_field(&fields, "platforms"),
        category,
        path: path_str,
        tags: list_field(&fields, "tags"),
        related_skills: list_field(&fields, "related_skills"),
        content: body.to_string(),
        enabled: true,
    })
}

fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Some(("", content));
    }
    let after_first = &trimmed[3..];
    let end = after_first.find("---")?;
    let frontmatter = after_first[..end].trim();
    let body = after_first[end + 3..].trim_start();
    Some((frontmatter, body))
}
