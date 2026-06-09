//! Session manager for multi-session CLI.
//!
//! Keeps multiple `BriocheShell` instances active in memory and
//! allows switching between them.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;

use brioche_shell_runtime::BriocheShell;

/// Manages multiple CLI sessions.
///
/// Each session is identified by a unique ID and has its own
/// shell, history, and state.
pub struct SessionManager {
    current: String,
    shells: BTreeMap<String, BriocheShell>,
}

impl SessionManager {
    /// Creates a new manager with an initial session.
    pub fn new(initial_id: impl Into<String>, initial_shell: BriocheShell) -> Self {
        let id = initial_id.into();
        let mut shells = BTreeMap::new();
        shells.insert(id.clone(), initial_shell);
        Self {
            current: id,
            shells,
        }
    }

    /// Reference to the current session's shell.
    pub fn current(&self) -> Option<&BriocheShell> {
        self.shells.get(&self.current)
    }

    /// Returns the current session ID.
    pub fn current_id(&self) -> &str {
        &self.current
    }

    /// Switches to another session.
    pub fn switch(&mut self, id: &str) {
        if self.shells.contains_key(id) {
            self.current = id.to_string();
        }
    }

    /// Inserts a new session.
    pub fn insert(&mut self, id: String, shell: BriocheShell) {
        self.shells.insert(id, shell);
    }

    /// Lists the IDs of all sessions in memory.
    pub fn list(&self) -> Vec<&String> {
        self.shells.keys().collect()
    }

    /// Access to a session by its ID.
    pub fn get(&self, id: &str) -> Option<&BriocheShell> {
        self.shells.get(id)
    }
}
