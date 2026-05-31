//! Politique de sandbox pour l'exécution de commandes shell.
//!
//! C'est de la **policy**, pas du mécanisme. Le mécanisme est
//! `execute_command` dans `tools::shell`. Cette politique décide
//! si une commande est autorisée.
//!
//! Refs: I-Shell-Runtime-OnlyIO

/// Handler de confirmation interactive pour les commandes shell.
///
/// Retourne `true` si l'utilisateur confirme l'exécution.
/// Le handler peut bloquer sur stdin ; il est appelé dans
/// `tokio::task::spawn_blocking` par l'outil.
pub type ConfirmHandler = std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Politique de sandbox pour les commandes shell.
#[derive(Clone, Debug)]
pub enum SandboxPolicy {
    /// Toute commande est autorisée (mode dangereux, exige confirmation).
    Permissive,
    /// Seules les commandes de la allow-list sont autorisées.
    /// Les autres déclenchent une confirmation interactive si un
    /// `ConfirmHandler` est configuré sur l'outil, sinon une erreur
    /// `ToolError::SandboxDenied`.
    AllowList(AllowList),
    /// Toute commande nécessite une confirmation interactive.
    /// Le handler doit être configuré sur l'outil ; sinon retourne
    /// une erreur en mode headless.
    Interactive,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::AllowList(AllowList::default())
    }
}

/// Liste explicite de commandes autorisées.
#[derive(Clone, Debug)]
pub struct AllowList {
    commands: std::collections::BTreeSet<String>,
}

impl AllowList {
    pub fn new() -> Self {
        Self {
            commands: std::collections::BTreeSet::new(),
        }
    }

    pub fn with_command(mut self, cmd: &str) -> Self {
        self.commands.insert(cmd.to_string());
        self
    }

    pub fn is_allowed(&self, command: &str) -> bool {
        let first_word = command.split_whitespace().next().unwrap_or("").trim();
        self.commands.contains(first_word)
    }
}

impl Default for AllowList {
    fn default() -> Self {
        Self::new()
            .with_command("ls")
            .with_command("cat")
            .with_command("grep")
            .with_command("find")
            .with_command("git")
            .with_command("cargo")
            .with_command("rustc")
            .with_command("pwd")
            .with_command("echo")
            .with_command("head")
            .with_command("tail")
            .with_command("wc")
    }
}
