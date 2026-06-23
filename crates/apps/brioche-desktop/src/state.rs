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
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use brioche_core::ChatMessage;
use brioche_provider_openai::{LlmChunk, OpenAiLlmClient};
use brioche_shell_persistence::{
    ExtensionRegistry, RedbStorage, SessionStore, Settings, new_session_store,
};
use brioche_shell_runtime::BriocheShell;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};

use crate::commands::shell::{DesktopConfig, ShellFactory, build_shell};

/// Shared history mirror type.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub type SharedHistory = Arc<RwLock<Vec<ChatMessage>>>;

/// A session entry: shell + LLM client + history mirror + chunk receiver.
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

/// Persistent metadata for a session.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Session identifier.
    pub id: String,
    /// Creation timestamp in seconds since the UNIX epoch.
    pub created_at: u64,
    /// Workspace / working directory associated with the session.
    pub workspace: String,
}

impl SessionMetadata {
    /// Creates metadata for a new session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1). Reads system clock.
    ///
    /// # Panic / Safety
    /// Never panics. Timestamps before the UNIX epoch are clamped to 0.
    pub fn new(id: impl Into<String>, workspace: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            created_at: system_time_secs(),
            workspace: workspace.into(),
        }
    }
}

fn system_time_secs() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0,
    }
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
    /// Persistent metadata for all known sessions.
    metadata: BTreeMap<String, SessionMetadata>,
}

impl SessionManager {
    /// Creates a new manager with an initial session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(S + N) where S is in-memory session insertion and N is metadata file size on disk.
    /// Performs blocking disk write.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if the metadata store cannot be written.
    pub fn new(
        initial_id: impl Into<String>,
        initial_shell: BriocheShell,
        initial_llm: OpenAiLlmClient,
        initial_history: SharedHistory,
        initial_llm_rx: broadcast::Receiver<LlmChunk>,
        workspace: &str,
    ) -> Result<Self, String> {
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
        let metadata = match Self::load_metadata() {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!("Failed to load session metadata, using defaults: {err}");
                BTreeMap::new()
            }
        };
        let mut manager = Self {
            current: id.clone(),
            sessions,
            metadata,
        };
        manager.insert_metadata(SessionMetadata::new(&id, workspace))?;
        Ok(manager)
    }

    /// Reference to the current session's shell.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of sessions.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn current_shell(&self) -> Option<&BriocheShell> {
        self.sessions.get(&self.current).map(|e| &e.shell)
    }

    /// Reference to the current session's LLM client.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of sessions.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn current_llm(&self) -> Option<&OpenAiLlmClient> {
        self.sessions.get(&self.current).map(|e| &e.llm)
    }

    /// Returns the current session ID.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1).
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn current_id(&self) -> &str {
        &self.current
    }

    /// Switches to another session if it exists.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of sessions.
    ///
    /// # Panic / Safety
    /// Never panics. Silently ignores unknown session IDs.
    pub fn switch(&mut self, id: &str) {
        if self.sessions.contains_key(id) {
            self.current = id.to_string();
        }
    }

    /// Inserts a new in-memory session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of sessions.
    ///
    /// # Panic / Safety
    /// Never panics.
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
    ///
    /// # Complexity
    /// O(S) where S is the number of sessions.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn list(&self) -> Vec<&String> {
        self.sessions.keys().collect()
    }

    /// Access to a session by its ID.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of sessions.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn get(&self, id: &str) -> Option<&SessionEntry> {
        self.sessions.get(id)
    }

    /// Takes the LLM chunk receiver from the current session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of sessions.
    ///
    /// # Panic / Safety
    /// Never panics. Returns `None` if the receiver was already taken.
    pub fn take_llm_rx(&mut self) -> Option<broadcast::Receiver<LlmChunk>> {
        self.sessions
            .get_mut(&self.current)
            .and_then(|e| e.llm_rx.take())
    }

    /// Returns metadata for a session, or `None` if the session is unknown.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of tracked sessions.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn metadata(&self, id: &str) -> Option<SessionMetadata> {
        self.metadata.get(id).cloned()
    }

    /// Inserts or updates metadata for a session and persists the store.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of tracked sessions. Performs blocking disk write.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if the metadata store cannot be written.
    pub fn insert_metadata(&mut self, metadata: SessionMetadata) -> Result<(), String> {
        self.metadata.insert(metadata.id.clone(), metadata);
        Self::save_metadata(&self.metadata)
    }

    /// Removes metadata for a session and persists the store.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(log S) where S is the number of tracked sessions. Performs blocking disk write.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if the metadata store cannot be written.
    pub fn remove_metadata(&mut self, id: &str) -> Result<(), String> {
        self.metadata.remove(id);
        Self::save_metadata(&self.metadata)
    }

    /// Loads metadata from disk.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the size of the metadata file on disk. Performs blocking disk read.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if the file cannot be read or parsed.
    pub fn load_metadata() -> Result<BTreeMap<String, SessionMetadata>, String> {
        let path = Self::metadata_path();
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read session metadata: {e}"))?;
        serde_json::from_str::<BTreeMap<String, SessionMetadata>>(&data)
            .map_err(|e| format!("Failed to parse session metadata: {e}"))
    }

    fn save_metadata(metadata: &BTreeMap<String, SessionMetadata>) -> Result<(), String> {
        let path = Self::metadata_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create session metadata dir: {e}"))?;
        }
        let data = serde_json::to_string_pretty(metadata)
            .map_err(|e| format!("Failed to serialize session metadata: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write session metadata: {e}"))
    }

    fn metadata_path() -> PathBuf {
        let config_dir = match dirs::config_dir() {
            Some(d) => d,
            None => std::env::temp_dir(),
        };
        config_dir.join("brioche-desktop").join("sessions.json")
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
    /// Loaded desktop extensions (context engine, memory, tools, skills, ...).
    pub extensions: RwLock<ExtensionRegistry>,
    /// Last note emitted by the active context engine (shown in the footer).
    pub last_context_note: Arc<Mutex<Option<String>>>,
}

impl DesktopState {
    /// Creates state without an active session.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1) plus storage initialization. Attempts two RedbStorage paths.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if storage cannot be initialized at any path.
    pub fn new() -> Result<Self, String> {
        Self::new_with_path("/tmp/brioche-desktop.redb")
    }

    /// Creates state with a custom redb path (for testing).
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1) plus storage initialization. Falls back to a temporary path on failure.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if storage cannot be initialized at any path.
    pub fn new_with_path(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let config = DesktopConfig::default();
        let store = new_session_store();
        let redb = Self::init_redb(path.as_ref(), store.clone())
            .map_err(|e| format!("Failed to initialize storage: {}", e))?;

        let extensions = ExtensionRegistry::default_set();
        let last_context_note = Arc::new(Mutex::new(None));
        let factory = ShellFactory {
            redb: redb.clone(),
            store: store.clone(),
            config: config.clone(),
            extensions: extensions.clone(),
            settings: Settings::load(),
            last_context_note: Arc::clone(&last_context_note),
        };

        Ok(Self {
            manager: RwLock::new(None),
            config: RwLock::new(config),
            factory: RwLock::new(factory),
            extensions: RwLock::new(extensions),
            last_context_note,
        })
    }

    /// Attempts to initialize RedbStorage with fallback paths.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn init_redb(primary: &std::path::Path, store: SessionStore) -> Result<RedbStorage, String> {
        if let Ok(r) = RedbStorage::new(primary, store.clone()) {
            return Ok(r);
        }
        let temp_path = std::env::temp_dir().join("brioche-desktop.redb");
        if let Ok(r) = RedbStorage::new(&temp_path, store.clone()) {
            return Ok(r);
        }
        tracing::error!("Failed to initialize RedbStorage at all paths");
        Err("Cannot initialize storage".to_string())
    }

    /// Ensures the session manager is initialized.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(S + M) where S is shell creation and M is memory provider initialization.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if shell build or memory provider initialization fails.
    pub async fn ensure_manager(&self) -> Result<(), String> {
        let mut mgr = self.manager.write().await;
        if mgr.is_none() {
            let factory = self.factory.read().await.clone();
            let handle = build_shell("desktop-session", &factory);
            let workspace = factory.settings.working_dir();
            Self::initialize_memory_providers(&factory, "desktop-session", &workspace)?;
            *mgr = Some(SessionManager::new(
                "desktop-session",
                handle.shell,
                handle.llm,
                handle.history,
                handle.llm_rx,
                &workspace,
            )?);
        }
        Ok(())
    }

    /// Notifies all registered memory providers of a new session context.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(M) where M is the number of memory providers. Performs blocking I/O per provider.
    ///
    /// # Panic / Safety
    /// Never panics. Returns Err if any provider fails to initialize.
    pub fn initialize_memory_providers(
        factory: &crate::commands::shell::ShellFactory,
        session_id: &str,
        workspace: &str,
    ) -> Result<(), String> {
        let ctx = brioche_shell_persistence::MemorySessionContext {
            session_id: session_id.into(),
            workspace: workspace.into(),
            user_id: None,
            agent_id: None,
        };
        for provider in factory.extensions.memory_providers() {
            provider.initialize(ctx.clone())?;
        }
        Ok(())
    }
}
