//! Skills management for the desktop app.
//!
//! Scans `~/.hermes/skills/` for SKILL.md files and exposes them
//! to the frontend as structured skill descriptors.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A parsed skill descriptor.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    /// Skill name (from frontmatter).
    pub name: String,
    /// Short description.
    pub description: String,
    /// Version string.
    pub version: String,
    /// Author name.
    pub author: String,
    /// License.
    pub license: String,
    /// Supported platforms.
    pub platforms: Vec<String>,
    /// Category (derived from directory structure).
    pub category: String,
    /// Full path to the skill directory.
    pub path: String,
    /// Tags from metadata.
    pub tags: Vec<String>,
    /// Related skills.
    pub related_skills: Vec<String>,
    /// Raw markdown content (without frontmatter).
    pub content: String,
}

/// Request to invoke a skill.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Deserialize)]
pub struct SkillInvokeRequest {
    /// Skill name to invoke.
    pub name: String,
    /// Arguments for the skill (key-value pairs).
    pub args: BTreeMap<String, String>,
}

/// Result of invoking a skill.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize)]
pub struct SkillInvokeResult {
    /// Whether the invocation succeeded.
    pub success: bool,
    /// Output message or error.
    pub message: String,
}

/// Scans the Hermes skills directory and returns all discovered skills.
///
/// Looks for `SKILL.md` files under `~/.hermes/skills/`. Each skill
/// directory may contain a category subdirectory (e.g., `github/`,
/// `productivity/`) followed by the skill name directory.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn scan_skills() -> Vec<Skill> {
    let skills_dir = skills_dir();
    if !skills_dir.exists() {
        return Vec::new();
    }

    let mut skills = Vec::new();
    scan_dir(&skills_dir, &skills_dir, &mut skills);
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

fn scan_dir(root: &Path, current: &Path, out: &mut Vec<Skill>) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Check if this directory contains a SKILL.md
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                if let Some(skill) = parse_skill(&skill_md, root) {
                    out.push(skill);
                }
            } else {
                // Recurse into subdirectories
                scan_dir(root, &path, out);
            }
        }
    }
}

#[allow(clippy::manual_unwrap_or_default)]
fn parse_skill(skill_md: &Path, root: &Path) -> Option<Skill> {
    let content = std::fs::read_to_string(skill_md).ok()?;
    let (frontmatter, body) = split_frontmatter(&content)?;

    let mut name = String::new();
    let mut description = String::new();
    let mut version = String::new();
    let mut author = String::new();
    let mut license = String::new();
    let mut platforms = Vec::new();
    let tags = Vec::new();
    let related_skills = Vec::new();

    // Simple YAML-like parser for frontmatter
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
                _ => {}
            }
        }
    }

    if name.is_empty() {
        // Fallback: use directory name
        name = match skill_md
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
        {
            Some(n) => n,
            None => String::new(),
        };
    }

    // Derive category from directory structure
    let category = match skill_md.parent().and_then(|p| p.parent()).and_then(|p| {
        let rel = p.strip_prefix(root).ok()?;
        rel.components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
    }) {
        Some(c) => c,
        None => "uncategorized".to_string(),
    };

    let path_str = match skill_md.parent().map(|p| p.to_string_lossy().to_string()) {
        Some(p) => p,
        None => String::new(),
    };

    Some(Skill {
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

/// Returns the Hermes skills directory path.
fn skills_dir() -> PathBuf {
    let home = match dirs::home_dir() {
        Some(d) => d,
        None => std::env::temp_dir(),
    };
    home.join(".hermes").join("skills")
}

/// Reads the full content of a skill's SKILL.md file.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn read_skill_content(name: &str) -> Option<String> {
    let skills = scan_skills();
    let skill = skills.iter().find(|s| s.name == name)?;
    let skill_md = PathBuf::from(&skill.path).join("SKILL.md");
    std::fs::read_to_string(skill_md).ok()
}

/// Reads a linked file from a skill directory.
///
/// Common linked files: `references/*.md`, `templates/*.yaml`, `scripts/*.py`.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn read_skill_file(skill_name: &str, file_path: &str) -> Option<String> {
    let skills = scan_skills();
    let skill = skills.iter().find(|s| s.name == skill_name)?;
    let full_path = PathBuf::from(&skill.path).join(file_path);
    std::fs::read_to_string(full_path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_parses_yaml() {
        let content =
            "---\nname: test-skill\ndescription: A test skill\n---\n\n# Body\nSome content.";
        let result = split_frontmatter(content);
        assert!(
            result.is_some(),
            "split_frontmatter should succeed on valid frontmatter"
        );
        let (fm, body) = match result {
            Some(r) => r,
            None => return,
        };
        assert!(fm.contains("name: test-skill"));
        assert!(body.contains("# Body"));
    }

    #[test]
    fn split_frontmatter_no_frontmatter() {
        let content = "# Just a markdown file\nNo frontmatter here.";
        let result = split_frontmatter(content);
        assert!(
            result.is_some(),
            "split_frontmatter should succeed even without frontmatter"
        );
        let (fm, body) = match result {
            Some(r) => r,
            None => return,
        };
        assert!(fm.is_empty());
        assert_eq!(body, content);
    }
}
