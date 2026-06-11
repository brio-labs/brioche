//! Sandbox policy for shell command execution.
//!
//! This is **policy**, not mechanism. The mechanism is
//! `execute_command` in `tools::shell`. This policy decides
//! whether a command is allowed.
//!
//! Refs: I-Shell-Runtime-OnlyIO

/// Interactive confirmation handler for shell commands.
///
/// Returns `true` if the user confirms execution.
/// The handler may block on stdin; it is called inside
/// `tokio::task::spawn_blocking` by the tool.
pub type ConfirmHandler = std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Sandbox policy for shell commands.
/// Refs: SPECS.md §Book III-C
#[derive(Clone, Debug)]
pub enum SandboxPolicy {
    /// Any command is allowed (dangerous mode, requires confirmation).
    Permissive,
    /// Only commands in the allow-list are allowed.
    /// Others trigger an interactive confirmation if a
    /// `ConfirmHandler` is configured on the tool, otherwise a
    /// `ToolError::SandboxDenied` error.
    AllowList(AllowList),
    /// Every command requires interactive confirmation.
    /// The handler must be configured on the tool; otherwise returns
    /// an error in headless mode.
    Interactive,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::AllowList(AllowList::default())
    }
}

/// Explicit list of allowed commands.
/// Refs: SPECS.md §Book III-C
#[derive(Clone, Debug)]
pub struct AllowList {
    commands: std::collections::BTreeSet<String>,
}

impl AllowList {
    /// Creates an empty allow-list.
    /// Refs: SPECS.md §Book III-C
    pub fn new() -> Self {
        Self {
            commands: std::collections::BTreeSet::new(),
        }
    }

    /// Adds a command to the allow-list.
    /// Refs: SPECS.md §Book III-C
    pub fn with_command(mut self, cmd: &str) -> Self {
        self.commands.insert(cmd.to_string());
        self
    }

    /// Checks whether a command is in the allow-list.
    /// Refs: SPECS.md §Book III-C
    pub fn is_allowed(&self, command: &str) -> bool {
        let first_word = command.split_whitespace().next().map_or("", |s| s).trim();
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
