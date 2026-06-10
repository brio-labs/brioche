//! Session manager for multi-session CLI — Book III §3.2.
//!
//! Keeps multiple `BriocheShell` instances active in memory and
//! allows switching between them.
//!
//! ## Invariants upheld
//! - I-Eco-OrderedCollections: Uses `BTreeMap` for deterministic session ordering.
//! - I-Shell-Runtime-OnlyIO: Shell-side state only; no Core mutation.
//!
//! Refs: I-Shell-Runtime-OnlyIO, I-Eco-OrderedCollections

use std::collections::BTreeMap;

use brioche_shell_runtime::BriocheShell;

/// Manages multiple CLI sessions.
///
/// Each session is identified by a unique ID and has its own
/// shell, history, and state.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct SessionManager {
    current: String,
    shells: BTreeMap<String, BriocheShell>,
}

impl SessionManager {
    /// Creates a new manager with an initial session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
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
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn current(&self) -> Option<&BriocheShell> {
        self.shells.get(&self.current)
    }

    /// Returns the current session ID.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn current_id(&self) -> &str {
        &self.current
    }

    /// Switches to another session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn switch(&mut self, id: &str) {
        if self.shells.contains_key(id) {
            self.current = id.to_string();
        }
    }

    /// Inserts a new session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn insert(&mut self, id: String, shell: BriocheShell) {
        self.shells.insert(id, shell);
    }

    /// Lists the IDs of all sessions in memory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn list(&self) -> Vec<&String> {
        self.shells.keys().collect()
    }

    /// Access to a session by its ID.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn get(&self, id: &str) -> Option<&BriocheShell> {
        self.shells.get(id)
    }
}
