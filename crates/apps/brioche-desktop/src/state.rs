//! Desktop application state.
//!
//! `DesktopState` holds a `SessionManager` (multi-session), a `ShellFactory`
//! for creating new shells, and routes messages between the frontend and
//! the shell runtime.
//!
//! # Design
//! - Multi-session via `SessionManager` (like `brioche-reedline`).
//! - Each session has a `BriocheShell` + `OpenAiLlmClient` + broadcast receiver.
//! - LLM chunks are streamed to the frontend via Tauri events.
//! - Slash commands are parsed and executed in the backend.
//! - Persistence via `RedbStorage` and `SessionStore`.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::collections::BTreeMap;
use std::sync::Arc;

use brioche_core::ChatMessage;
use brioche_provider_openai::{LlmChunk, OpenAiLlmClient};
use brioche_shell_persistence::{RedbStorage, SessionStore, new_session_store};
use brioche_shell_runtime::BriocheShell;
use tokio::sync::{RwLock, broadcast};

use crate::commands::shell::{DesktopConfig, ShellFactory, build_shell};

/// Shared history mirror type.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub type SharedHistory = Arc<RwLock<Vec<ChatMessage>>>;

/// A session entry: shell + LLM client + history mirror + chunk receiver.
///
/// The `llm_rx` field is an `Option` because it is taken when spawning
/// the forwarder task. Once taken, the receiver is owned by the task.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct SessionEntry {
    /// The shell instance.
    pub shell: BriocheShell,
    /// LLM client for pushing messages.
    pub llm: OpenAiLlmClient,
    /// Shared history mirror (for get_messages).
    pub history: SharedHistory,
    /// Broadcast receiver for LLM chunks.
    ///
    /// `None` after the forwarder task has been spawned.
    pub llm_rx: Option<broadcast::Receiver<LlmChunk>>,
}

/// Multi-session manager for the desktop.
///
/// Like `brioche_reedline::SessionManager` but stores `SessionEntry`
/// (shell + LLM client) instead of just `BriocheShell`.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct SessionManager {
    current: String,
    /// All sessions keyed by ID.
    pub sessions: BTreeMap<String, SessionEntry>,
}

impl SessionManager {
    /// Creates a new manager with an initial session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new(
        initial_id: impl Into<String>,
        initial_shell: BriocheShell,
        initial_llm: OpenAiLlmClient,
        initial_history: SharedHistory,
        initial_llm_rx: broadcast::Receiver<LlmChunk>,
    ) -> Self {
        let id = initial_id.into();
        let mut sessions = BTreeMap::new();
        sessions.insert(
            id.clone(),
            SessionEntry {
                shell: initial_shell,
                llm: initial_llm,
                history: initial_history,
                llm_rx: Some(initial_llm_rx),
            },
        );
        Self {
            current: id,
            sessions,
        }
    }

    /// Reference to the current session's shell.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn current_shell(&self) -> Option<&BriocheShell> {
        self.sessions.get(&self.current).map(|e| &e.shell)
    }

    /// Reference to the current session's LLM client.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn current_llm(&self) -> Option<&OpenAiLlmClient> {
        self.sessions.get(&self.current).map(|e| &e.llm)
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
        if self.sessions.contains_key(id) {
            self.current = id.to_string();
        }
    }

    /// Inserts a new session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn insert(
        &mut self,
        id: String,
        shell: BriocheShell,
        llm: OpenAiLlmClient,
        history: SharedHistory,
        llm_rx: broadcast::Receiver<LlmChunk>,
    ) {
        self.sessions.insert(
            id,
            SessionEntry {
                shell,
                llm,
                history,
                llm_rx: Some(llm_rx),
            },
        );
    }

    /// Lists the IDs of all sessions in memory.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn list(&self) -> Vec<&String> {
        self.sessions.keys().collect()
    }

    /// Access to a session by its ID.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn get(&self, id: &str) -> Option<&SessionEntry> {
        self.sessions.get(id)
    }

    /// Takes the LLM chunk receiver from the current session.
    ///
    /// Returns `None` if the session doesn't exist or the receiver
    /// has already been taken.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn take_llm_rx(&mut self) -> Option<broadcast::Receiver<LlmChunk>> {
        self.sessions
            .get_mut(&self.current)
            .and_then(|e| e.llm_rx.take())
    }
}

/// Desktop application state.
///
/// Holds the session manager, configuration, and shared dependencies
/// for creating new shells. All fields are behind async locks for
/// thread-safe access.
///
/// The session manager is initialized lazily on first access so that
/// `build_shell` (which spawns Tokio tasks) runs inside an async
/// runtime context.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub struct DesktopState {
    /// Multi-session manager (current session + all entries).
    /// `None` until first access triggers lazy initialization.
    pub manager: RwLock<Option<SessionManager>>,
    /// CLI-style configuration for the OpenAI provider.
    pub config: RwLock<DesktopConfig>,
    /// Factory for creating new shells (shared dependencies).
    pub factory: RwLock<ShellFactory>,
}

impl DesktopState {
    /// Creates state without an active session.
    ///
    /// The initial shell is built lazily on the first IPC command
    /// so that `tokio::spawn` runs inside Tauri's async runtime.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new() -> Result<Self, String> {
        Self::new_with_path("/tmp/brioche-desktop.redb")
    }
}

impl DesktopState {
    /// Creates state with a custom redb path (for testing).
    ///
    /// Returns an error if storage cannot be initialized at any path.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new_with_path(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let config = DesktopConfig::default();
        let store = new_session_store();
        let redb = Self::init_redb(path.as_ref(), store.clone())
            .map_err(|e| format!("Failed to initialize storage: {}", e))?;

        let factory = ShellFactory {
            redb: redb.clone(),
            store: store.clone(),
            config: config.clone(),
        };

        Ok(Self {
            manager: RwLock::new(None),
            config: RwLock::new(config),
            factory: RwLock::new(factory),
        })
    }

    /// Attempts to initialize RedbStorage with fallback paths.
    ///
    /// Tries primary, then fallback, then temp dir. Returns the first
    /// successful storage or an error if all paths fail.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn init_redb(primary: &std::path::Path, store: SessionStore) -> Result<RedbStorage, String> {
        if let Ok(r) = RedbStorage::new(primary, store.clone()) {
            return Ok(r);
        }
        if let Ok(r) = RedbStorage::new("/tmp/brioche-desktop-fallback.redb", store.clone()) {
            return Ok(r);
        }
        let temp_path = std::env::temp_dir().join("brioche-desktop.redb");
        if let Ok(r) = RedbStorage::new(&temp_path, store.clone()) {
            return Ok(r);
        }
        tracing::error!("Failed to initialize RedbStorage at all paths");
        // Final attempt with fresh store
        match RedbStorage::new("/tmp/brioche-desktop.redb", new_session_store()) {
            Ok(r) => Ok(r),
            Err(_) => {
                tracing::error!("Fatal: cannot create any storage");
                Err("Cannot initialize storage".to_string())
            }
        }
    }

    /// Ensures the session manager is initialized.
    ///
    /// Call this at the top of every IPC command that needs sessions.
    /// This is where `build_shell` is called — inside an async context
    /// so that `tokio::spawn` works.
    ///
    /// The LLM chunk receiver is stored in the session entry and is
    /// consumed later (e.g., in `send_message`) when an `AppHandle`
    /// is available to emit Tauri events.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub async fn ensure_manager(&self) -> Result<(), String> {
        let mut mgr = self.manager.write().await;
        if mgr.is_none() {
            let config = self.config.read().await.clone();
            let factory = self.factory.read().await.clone();
            let handle = build_shell("desktop-session", &config, factory.redb, factory.store);

            *mgr = Some(SessionManager::new(
                "desktop-session",
                handle.shell,
                handle.llm,
                handle.history,
                handle.llm_rx,
            ));
        }
        Ok(())
    }
}
