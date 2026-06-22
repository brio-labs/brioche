//! Skills management.
//!
//! This module is a compatibility wrapper around the extension-system
//! [`SkillRegistry`]. New code should prefer using the provider directly via
//! [`crate::extensions::ExtensionRegistry`].
//!
//! Refs: I-Shell-Runtime-OnlyIO

use crate::extensions::skill_provider::SkillProvider;
pub use crate::extensions::skill_provider::{SkillDescriptor as Skill, SkillRegistry};

/// Scans the Hermes skills directory and returns all discovered skills.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(K) where K is the number of skills. Performs disk I/O directory scanning.
///
/// # Panic / Safety
/// Never panics. Returns empty vector if scanning fails.
pub fn scan_skills() -> Vec<Skill> {
    let registry = SkillRegistry::default();
    registry.list().map_or(Vec::new(), |skills| skills)
}

/// Reads the full content of a skill's SKILL.md file.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(S) where S is the size of the SKILL.md file on disk. Performs disk I/O.
///
/// # Panic / Safety
/// Never panics. Returns None if reading fails.
pub fn read_skill_content(name: &str) -> Option<String> {
    let registry = SkillRegistry::default();
    registry.read_content(name).ok()
}

/// Reads a linked file from a skill directory.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(F) where F is the size of the linked file on disk. Performs disk I/O.
///
/// # Panic / Safety
/// Never panics. Returns None if reading fails.
pub fn read_skill_file(skill_name: &str, file_path: &str) -> Option<String> {
    let registry = SkillRegistry::default();
    registry.read_file(skill_name, file_path).ok()
}
