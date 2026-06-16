//! Modular skill provider extension point.
//!
//! Skills are prompt packages that can be enabled, disabled, tagged and sorted.
//! The default provider scans the Hermes skills directory; future providers can
//! load skills from a workspace, a git repository or a remote registry.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use super::{ExtensionMetadata, PanelSlot};
use serde::{Deserialize, Serialize};

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
    fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<(), String>;
}

/// Default skill registry that scans `~/.hermes/skills/`.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug)]
pub struct SkillRegistry {
    enabled: Vec<String>,
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self {
            enabled: Self::load_enabled(),
        }
    }
}

impl SkillRegistry {
    /// Returns the Hermes skills directory.
    fn skills_dir() -> std::path::PathBuf {
        let home = match dirs::home_dir() {
            Some(d) => d,
            None => std::env::temp_dir(),
        };
        home.join(".hermes").join("skills")
    }

    /// Loads enabled state from disk.
    fn load_enabled() -> Vec<String> {
        let path = Self::enabled_path();
        if let Ok(data) = std::fs::read_to_string(&path)
            && let Ok(enabled) = serde_json::from_str::<Vec<String>>(&data)
        {
            return enabled;
        }
        Vec::new()
    }

    /// Saves enabled state to disk.
    fn save_enabled(&self) -> Result<(), String> {
        let path = Self::enabled_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create skills dir: {e}"))?;
        }
        let data = serde_json::to_string_pretty(&self.enabled)
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
        Ok(skills
            .into_iter()
            .map(|mut s| {
                s.enabled = self.enabled.contains(&s.name) || self.enabled.is_empty();
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

    fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<(), String> {
        if enabled {
            if !self.enabled.contains(&name.to_string()) {
                self.enabled.push(name.to_string());
            }
        } else {
            self.enabled.retain(|n| n != name);
        }
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

#[allow(clippy::manual_unwrap_or_default)]
fn parse_skill(skill_md: &std::path::Path, root: &std::path::Path) -> Option<SkillDescriptor> {
    let content = std::fs::read_to_string(skill_md).ok()?;
    let (frontmatter, body) = split_frontmatter(&content)?;

    let mut name = String::new();
    let mut description = String::new();
    let mut version = String::new();
    let mut author = String::new();
    let mut license = String::new();
    let mut platforms = Vec::new();
    let mut tags = Vec::new();
    let mut related_skills = Vec::new();

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key {
                "name" => name = value.to_string(),
                "description" => description = value.to_string(),
                "version" => version = value.to_string(),
                "author" => author = value.to_string(),
                "license" => license = value.to_string(),
                "platforms" => {
                    platforms = value
                        .trim_matches('[')
                        .trim_matches(']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "tags" => {
                    tags = value
                        .trim_matches('[')
                        .trim_matches(']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "related_skills" => {
                    related_skills = value
                        .trim_matches('[')
                        .trim_matches(']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }

    if name.is_empty() {
        name = match skill_md.parent().and_then(|p| p.file_name()) {
            Some(n) => n.to_string_lossy().to_string(),
            None => String::new(),
        };
    }

    let category = match skill_md.parent().and_then(|p| p.parent()) {
        Some(parent) => match parent.strip_prefix(root).ok().and_then(|rel| {
            rel.components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
        }) {
            Some(c) => c,
            None => "uncategorized".to_string(),
        },
        None => "uncategorized".to_string(),
    };

    let path_str = match skill_md.parent() {
        Some(p) => p.to_string_lossy().to_string(),
        None => String::new(),
    };

    Some(SkillDescriptor {
        name,
        description,
        version,
        author,
        license,
        platforms,
        category,
        path: path_str,
        tags,
        related_skills,
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
