//! Multi-conversation session manager for terminal agents.
//!
//! Keeps multiple `BriocheShell`s active in memory and allows
//! switching between them via slash commands (`/session load`).
//!
//! Refs: I-Shell-Session-NoSend

use brioche_shell_runtime::BriocheShell;
use std::collections::BTreeMap;

/// Session manager.
///
/// Each session is identified by a unique ID and has its own
/// `BriocheShell` (and therefore its own engine thread).
pub struct SessionManager {
    sessions: BTreeMap<String, BriocheShell>,
    current: String,
}

impl SessionManager {
    /// Creates a new manager with an initial session.
    pub fn new(initial_id: impl Into<String>, shell: BriocheShell) -> Self {
        let id = initial_id.into();
        let mut sessions = BTreeMap::new();
        sessions.insert(id.clone(), shell);
        Self {
            sessions,
            current: id,
        }
    }

    /// Reference to the current session's shell.
    ///
    /// Returns `None` only if the internal invariant is violated
    /// (the current session does not exist in the registry).
    pub fn current(&self) -> Option<&BriocheShell> {
        self.sessions.get(&self.current)
    }

    /// Switches the active session.
    ///
    /// Returns `true` if the session exists.
    pub fn switch(&mut self, id: &str) -> bool {
        if self.sessions.contains_key(id) {
            self.current = id.to_string();
            true
        } else {
            false
        }
    }

    /// Inserts a new session.
    pub fn insert(&mut self, id: impl Into<String>, shell: BriocheShell) {
        self.sessions.insert(id.into(), shell);
    }

    /// Lists the IDs of all sessions in memory.
    pub fn list(&self) -> Vec<&String> {
        self.sessions.keys().collect()
    }

    /// ID of the current session.
    pub fn current_id(&self) -> &str {
        &self.current
    }

    /// Accesses a session by its ID.
    pub fn get(&self, id: &str) -> Option<&BriocheShell> {
        self.sessions.get(id)
    }
}
