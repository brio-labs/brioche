//! Gestionnaire de sessions multi-conversation pour les agents terminal.
//!
//! Maintient plusieurs `BriocheShell` actifs en mémoire et permet
//! de basculer entre eux via des commandes slash (`/session load`).
//!
//! Refs: I-Shell-Session-NoSend

use brioche_shell_runtime::BriocheShell;
use std::collections::BTreeMap;

/// Gestionnaire de sessions.
///
/// Chaque session est identifiée par un ID unique et possède son
/// propre `BriocheShell` (donc son propre engine thread).
pub struct SessionManager {
    sessions: BTreeMap<String, BriocheShell>,
    current: String,
}

impl SessionManager {
    /// Crée un nouveau gestionnaire avec une session initiale.
    pub fn new(initial_id: impl Into<String>, shell: BriocheShell) -> Self {
        let id = initial_id.into();
        let mut sessions = BTreeMap::new();
        sessions.insert(id.clone(), shell);
        Self {
            sessions,
            current: id,
        }
    }

    /// Référence au shell de la session courante.
    ///
    /// Retourne `None` uniquement si l'invariant interne est violé
    /// (la session courante n'existe pas dans le registre).
    pub fn current(&self) -> Option<&BriocheShell> {
        self.sessions.get(&self.current)
    }

    /// Change la session active.
    ///
    /// Retourne `true` si la session existe.
    pub fn switch(&mut self, id: &str) -> bool {
        if self.sessions.contains_key(id) {
            self.current = id.to_string();
            true
        } else {
            false
        }
    }

    /// Insère une nouvelle session.
    pub fn insert(&mut self, id: impl Into<String>, shell: BriocheShell) {
        self.sessions.insert(id.into(), shell);
    }

    /// Liste les IDs de toutes les sessions en mémoire.
    pub fn list(&self) -> Vec<&String> {
        self.sessions.keys().collect()
    }

    /// ID de la session courante.
    pub fn current_id(&self) -> &str {
        &self.current
    }

    /// Accès à une session par son ID.
    pub fn get(&self, id: &str) -> Option<&BriocheShell> {
        self.sessions.get(id)
    }
}
