//! Shared `BriocheShell` construction routines.
//!
//! The [`ShellBuilder`] type captures the common steps between
//! `agent-terminal::shell_builder::build_shell` and
//! `brioche-desktop::commands::shell::build_shell`, so the same wiring logic
//! is not duplicated.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use std::sync::Arc;

use brioche_core::{BriocheEngine, ChatMessage, Session};
use brioche_plugin_kit::PluginBuilder;
use brioche_provider_openai::{
    HistoryTransform, LlmChunk, OpenAiConfig, OpenAiLlmClient, SharedHistory,
};
use brioche_shell_persistence::{
    PersistenceSubRoutineHydrator, RedbStorage, SessionHeadDTO, SessionStore, SessionStoreEntry,
};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, PersistenceMode, SessionCallback, ShellConfig,
};
use brioche_tools_system::SystemToolExecutor;
use tokio::sync::broadcast;

/// Default system prompt injected into the LLM client's history mirror.
///
/// This instructs the model to use available tools rather than emitting file
/// contents directly in its response text.
///
/// Refs: I-Shell-Runtime-OnlyIO
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful AI coding assistant with access to filesystem tools. \
CRITICAL RULES: \
1. When the user asks you to create, write, or modify ANY file, you MUST use the write_file tool. \
2. NEVER output file contents directly in your response text — ALWAYS use the write_file tool with the full content. \
3. For large files, you may use multiple write_file calls with append=true after the first call. \
4. Use read_file before modifying existing files. \
5. Use execute_command for shell commands. \
6. Use list_dir to explore directories. \
7. If you need to fetch content from the web, use fetch_url. \
8. After using write_file, the tool result will confirm success. Do not read the file back unless the user asks.";

/// Factory closure that builds the synchronous engine/session pair on the
/// engine thread.
///
/// Refs: I-Shell-Session-NoSend
type EngineFactory = Box<dyn FnOnce() -> (BriocheEngine, Session) + Send>;

/// Builder for constructing a fully wired [`BriocheShell`].
///
/// `agent-terminal` and `brioche-desktop` each configure their own tool
/// executor and optional history transform, then call [`ShellBuilder::build`]
/// to obtain the shell, LLM client, broadcast receiver, and shared history.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(T) where T is the number of tools registered. Spawns asynchronous tasks
/// in the background.
///
/// # Panic / Safety
/// Panics if called outside of a Tokio runtime context because [`ShellBuilder::build`]
/// spawns `tokio::spawn` tasks.
pub struct ShellBuilder {
    openai_config: OpenAiConfig,
    tick_interval_ms: u64,
    redb_storage: RedbStorage,
    session_store: SessionStore,
    tool_executor: SystemToolExecutor,
    history_transform: Option<HistoryTransform>,
    engine_factory: EngineFactory,
}

impl ShellBuilder {
    /// Starts a new builder for the given session and OpenAI configuration.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    /// Starts a new builder for the given session and OpenAI configuration.
    ///
    /// `redb_storage`, `session_store`, and `tool_executor` are required
    /// because every constructed shell needs them. Optional configuration
    /// (tick interval, history transform, engine factory) can be chained
    /// afterwards.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new(
        session_id: impl Into<String>,
        openai_config: OpenAiConfig,
        redb_storage: RedbStorage,
        session_store: SessionStore,
        tool_executor: SystemToolExecutor,
    ) -> Self {
        let engine_factory = default_session_factory(session_id.into(), redb_storage.clone());
        Self {
            openai_config,
            tick_interval_ms: 1000,
            redb_storage,
            session_store,
            tool_executor,
            history_transform: None,
            engine_factory: Box::new(engine_factory),
        }
    }

    /// Sets the shell tick interval in milliseconds.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn with_tick_interval_ms(mut self, tick_interval_ms: u64) -> Self {
        self.tick_interval_ms = tick_interval_ms;
        self
    }

    /// Sets an optional history transform applied before each LLM request.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn with_history_transform(mut self, transform: Option<HistoryTransform>) -> Self {
        self.history_transform = transform;
        self
    }

    /// Sets the factory closure that creates the engine and session.
    ///
    /// By default the factory hydrates persistence subroutines from the
    /// builder's Redb storage. Callers can override it with
    /// [`default_session_factory`] or [`session_factory_with_head`].
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn with_engine_factory<F>(mut self, factory: F) -> Self
    where
        F: FnOnce() -> (BriocheEngine, Session) + Send + 'static,
    {
        self.engine_factory = Box::new(factory);
        self
    }

    /// Consumes the builder and constructs a fully wired shell.
    ///
    /// This method must be called from within a Tokio runtime context because
    /// it spawns background tasks for the system prompt and tools schema.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn build(
        self,
    ) -> (
        BriocheShell,
        OpenAiLlmClient,
        broadcast::Receiver<LlmChunk>,
        SharedHistory,
    ) {
        let (llm_client, llm_rx, history) = OpenAiLlmClient::new(self.openai_config);

        if let Some(transform) = self.history_transform {
            llm_client.set_history_transform(Some(transform));
        }

        // Inject default system prompt into the LLM client's history mirror.
        let llm_for_prompt = llm_client.clone();
        let prompt = DEFAULT_SYSTEM_PROMPT.to_string();
        tokio::spawn(async move {
            llm_for_prompt
                .push_message(ChatMessage::System { content: prompt })
                .await;
        });

        // Push the tool schemas into the LLM client without blocking startup.
        let schemas = self.tool_executor.schema_json();
        let llm_for_schema = llm_client.clone();
        tokio::spawn(async move {
            llm_for_schema.set_tools_schema(schemas).await;
        });

        let effect_executor =
            DefaultEffectExecutor::new(self.tool_executor, llm_client.clone(), self.redb_storage)
                .with_history(Arc::clone(&history));

        let store_for_callback = Arc::clone(&self.session_store);
        let session_callback: SessionCallback = Box::new(move |session: &mut Session| {
            if let Ok(store) = store_for_callback.try_read()
                && let Some(existing) = store.get(&session.id)
            {
                session.persisted_msg_count = session
                    .persisted_msg_count
                    .max(existing.head.persisted_msg_count);
            }
            let head = brioche_shell_persistence::SessionHeadDTO::from_session(session);
            let entry = SessionStoreEntry {
                head,
                messages: session.history.clone(),
            };
            if let Ok(mut store) = store_for_callback.try_write() {
                store.insert(session.id.clone(), entry);
            }
        });

        let history_clone = Arc::clone(&history);
        let shell = BriocheShell::new(
            self.engine_factory,
            ShellConfig {
                engine_channel_capacity: 256,
                tick_interval_ms: self.tick_interval_ms,
                max_concurrent_effects: 32,
                persistence_mode: PersistenceMode::Async,
                transition_journal_enabled: false,
            },
            effect_executor,
            Some(session_callback),
        );

        (shell, llm_client, llm_rx, history_clone)
    }
}

/// Creates the standard engine/session factory used by most shells.
///
/// The factory hydrates persistence subroutines from the provided Redb
/// storage and creates a fresh session with the given id.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) closure creation. The actual engine build happens later on the
/// engine thread.
pub fn default_session_factory(
    session_id: impl Into<String>,
    redb_storage: RedbStorage,
) -> impl FnOnce() -> (BriocheEngine, Session) + Send {
    let session_id = session_id.into();
    move || {
        PluginBuilder::standard()
            .with_subroutine_hydrator(Box::new(PersistenceSubRoutineHydrator::new(redb_storage)))
            .build_with_session(session_id)
    }
}

/// Creates an engine/session factory that restores an initial session head.
///
/// If `initial_head` is provided, the session is reconstructed from it using
/// `initial_history`. Otherwise `initial_history` is assigned to the fresh
/// session's history.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) closure creation. Restoration happens later on the engine thread.
pub fn session_factory_with_head(
    session_id: impl Into<String>,
    redb_storage: RedbStorage,
    initial_history: Option<Vec<ChatMessage>>,
    initial_head: Option<SessionHeadDTO>,
) -> impl FnOnce() -> (BriocheEngine, Session) + Send {
    let session_id = session_id.into();
    move || {
        let (engine, mut session) = PluginBuilder::standard()
            .with_subroutine_hydrator(Box::new(PersistenceSubRoutineHydrator::new(redb_storage)))
            .build_with_session(&session_id);
        if let Some(head) = initial_head {
            #[allow(clippy::manual_unwrap_or_default)]
            let history = match initial_history {
                Some(hist) => hist,
                None => Vec::new(),
            };
            session = head.to_session(history);
        } else if let Some(hist) = initial_history {
            session.history = hist;
        }
        (engine, session)
    }
}
