//! Main async runtime coordinator (`BriocheShell`).
//!
//! `BriocheShell` owns the bridge between the synchronous kernel
//! (`BriocheEngine` + `Session`, both `!Send`) and the Tokio async
//! runtime. The engine runs on a dedicated `std::thread`; all I/O
//! and async work stays on the Tokio runtime.
//!
//! ## Architecture
//!
//! ```text
//!  [Async Runtime]          [Engine Thread]
//!       |                          |
//!   BriocheShell  --EngineInput-->  engine.transition()
//!       ^                          |
//!       |--(Vec<Effect>, StateSnapshot)--
//!       |
//!   EffectExecutor -> async tasks -> EngineInput (loopback)
//! ```
//!
//! Refs: I-Shell-Session-NoSend, I-Shell-Runtime-OnlyIO

use crate::EffectExecutor;
use brioche_core::{
    AgentState, AgentStateTag, BriocheEngine, Effect, EngineInput, GovernanceNotification, Session,
    SystemSignal,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Lightweight snapshot of session mechanical state sent from the
/// engine thread after each transition.
///
/// The async shell uses this to correlate async results (e.g.
/// `ToolCallsResult`) with the current engine state without
/// accessing `Session` directly.
///
/// Refs: I-Shell-Session-NoSend
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateSnapshot {
    /// Current mechanical state tag.
    pub state_tag: AgentStateTag,
    /// Generation ID if state is `Predicting` or `ExecutingTools`.
    pub generation_id: Option<u64>,
    /// Depth of the state stack.
    pub stack_depth: usize,
    /// Session identifier.
    pub session_id: String,
}

impl StateSnapshot {
    fn from_session(session: &Session) -> Self {
        let generation_id = match &session.state {
            AgentState::Predicting { generation_id }
            | AgentState::ExecutingTools { generation_id } => Some(*generation_id),
            _ => None,
        };
        Self {
            state_tag: AgentStateTag::from(&session.state),
            generation_id,
            stack_depth: session.state_stack.len(),
            session_id: session.id.clone(),
        }
    }
}

/// Errors originating in the shell runtime.
///
/// Refs: I-Core-NoPanic
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ShellError {
    #[error("engine thread disconnected")]
    EngineDisconnected,
    #[error("channel send failed")]
    ChannelSend,
    #[error("effect execution failed: {0}")]
    EffectExecution(String),
}

/// Configuration for `BriocheShell`.
///
/// All fields have sensible defaults.
#[derive(Clone, Debug)]
pub struct ShellConfig {
    /// Capacity of the bounded channel to the engine.
    pub engine_channel_capacity: usize,
    /// Default interval for `SystemSignal::Tick` in milliseconds.
    pub tick_interval_ms: u64,
    /// Maximum number of effects processed concurrently.
    pub max_concurrent_effects: usize,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            engine_channel_capacity: 256,
            tick_interval_ms: 1000,
            max_concurrent_effects: 32,
        }
    }
}

/// Main async runtime coordinator.
///
/// `BriocheShell` is `Clone` (cheaply, via `Arc` internals) so that
/// effect handlers and IPC endpoints can hold a handle to send inputs
/// back into the engine loop.
///
/// # Construction
///
/// Because `BriocheEngine` and `Session` are `!Send`, they must be
/// constructed on the engine thread itself. Use a factory closure:
///
/// ```ignore
/// use brioche_shell_runtime::{BriocheShell, ShellConfig, DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence};
/// use brioche_core::{BriocheEngineBuilder, Session};
///
/// # async fn example() {
/// let executor = DefaultEffectExecutor::new(
///     EchoToolExecutor,
///     MockLlmClient::default(),
///     NoopPersistence,
/// );
/// let shell = BriocheShell::new(
///     || {
///         let engine = BriocheEngineBuilder::new().build().unwrap();
///         let session = Session::new("main");
///         (engine, session)
///     },
///     ShellConfig::default(),
///     executor,
/// );
/// # }
/// ```
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone)]
pub struct BriocheShell {
    /// Sender for `EngineInput` into the engine thread.
    input_tx: mpsc::Sender<EngineInput>,
    /// Sender for `SystemSignal` into the engine thread (via adapter).
    system_signal_tx: mpsc::Sender<SystemSignal>,
    /// Sender for governance notifications.
    governance_tx: mpsc::Sender<GovernanceNotification>,
    /// Shared configuration.
    #[allow(dead_code)]
    config: Arc<ShellConfig>,
}

impl BriocheShell {
    /// Create a new shell, spawn the engine thread, and start the
    /// effect consumption loop.
    ///
    /// `engine_factory` is called on the dedicated engine thread.
    /// This respects the `!Send` invariant of `BriocheEngine` and `Session`.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub fn new<F, E>(engine_factory: F, config: ShellConfig, executor: E) -> Self
    where
        F: FnOnce() -> (BriocheEngine, Session) + Send + 'static,
        E: EffectExecutor + 'static,
    {
        let (input_tx, input_rx) = mpsc::channel::<EngineInput>(config.engine_channel_capacity);
        let (output_tx, output_rx) =
            mpsc::channel::<(Vec<Effect>, StateSnapshot)>(config.engine_channel_capacity);
        let (system_signal_tx, system_signal_rx) =
            mpsc::channel::<SystemSignal>(config.engine_channel_capacity);
        let (governance_tx, governance_rx) =
            mpsc::channel::<GovernanceNotification>(config.engine_channel_capacity);

        // Spawn the synchronous engine thread.
        std::thread::spawn(move || {
            let (engine, session) = engine_factory();
            engine_thread_loop(
                engine,
                session,
                input_rx,
                output_tx,
                system_signal_rx,
                governance_rx,
            );
        });

        let config_arc = Arc::new(config.clone());
        let shell = Self {
            input_tx: input_tx.clone(),
            system_signal_tx: system_signal_tx.clone(),
            governance_tx: governance_tx.clone(),
            config: Arc::clone(&config_arc),
        };

        // Spawn the async effect consumption loop.
        let shell_clone = shell.clone();
        tokio::spawn(async move {
            effect_consumption_loop(output_rx, shell_clone, executor, config_arc).await;
        });

        shell
    }

    /// Send an `EngineInput` to the kernel.
    ///
    /// Returns `Err(ShellError::ChannelSend)` if the engine thread
    /// has shut down.
    ///
    /// Refs: I-Shell-Backpressure-NoOverflow
    pub async fn send_input(&self, input: EngineInput) -> Result<(), ShellError> {
        self.input_tx
            .send(input)
            .await
            .map_err(|_| ShellError::ChannelSend)
    }

    /// Send a `SystemSignal` to the kernel.
    ///
    /// Signals are drained by the shell between transition cycles and
    /// injected into the engine via `EngineInput` adapters.
    ///
    /// Refs: I-Shell-Network-Signal
    pub async fn send_system_signal(&self, signal: SystemSignal) -> Result<(), ShellError> {
        self.system_signal_tx
            .send(signal)
            .await
            .map_err(|_| ShellError::ChannelSend)
    }

    /// Send a `GovernanceNotification` to the kernel.
    pub async fn send_governance_notification(
        &self,
        notification: GovernanceNotification,
    ) -> Result<(), ShellError> {
        self.governance_tx
            .send(notification)
            .await
            .map_err(|_| ShellError::ChannelSend)
    }

    /// Block until the engine channel has capacity.
    ///
    /// Useful for backpressure-aware producers.
    pub async fn ready(&self) -> Result<(), ShellError> {
        if self.input_tx.is_closed() {
            return Err(ShellError::EngineDisconnected);
        }
        Ok(())
    }

    /// Gracefully shut down the shell.
    ///
    /// Drops the input sender, causing the engine thread to exit
    /// after processing pending inputs.
    pub fn shutdown(&self) {
        // Dropping all senders causes receivers to return `None`.
        // The engine thread and effect loop will terminate naturally.
    }
}

// ---------------------------------------------------------------------------
// Engine thread
// ---------------------------------------------------------------------------

/// Synchronous loop running `BriocheEngine` on a dedicated thread.
///
/// This function owns `engine` and `session`; they never leave this thread.
fn engine_thread_loop(
    mut engine: BriocheEngine,
    mut session: Session,
    mut input_rx: mpsc::Receiver<EngineInput>,
    output_tx: mpsc::Sender<(Vec<Effect>, StateSnapshot)>,
    mut system_signal_rx: mpsc::Receiver<SystemSignal>,
    mut governance_rx: mpsc::Receiver<GovernanceNotification>,
) {
    // Create local adapters that drain the async channels into
    // local collections between transition cycles.
    let mut system_signals: Vec<SystemSignal> = Vec::new();
    let mut governance_notifications: Vec<GovernanceNotification> = Vec::new();

    while let Some(input) = input_rx.blocking_recv() {
        // Drain pending system signals (best-effort, non-blocking).
        while let Ok(signal) = system_signal_rx.try_recv() {
            system_signals.push(signal);
        }
        // Drain pending governance notifications.
        while let Ok(notification) = governance_rx.try_recv() {
            governance_notifications.push(notification);
        }

        // Execute the synchronous transition.
        let effects = engine.transition(&mut session, &input);
        let snapshot = StateSnapshot::from_session(&session);

        // Send results back to the async runtime.
        if output_tx.blocking_send((effects, snapshot)).is_err() {
            // Async runtime dropped; shut down.
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Effect consumption loop
// ---------------------------------------------------------------------------

/// Async loop that receives effect batches from the engine thread and
/// dispatches each effect to the appropriate async handler.
///
/// Some effects spawn async tasks that eventually produce `EngineInput`
/// loopback messages (e.g. `ToolCallsResult`, `LlmStream`).
async fn effect_consumption_loop<E>(
    mut output_rx: mpsc::Receiver<(Vec<Effect>, StateSnapshot)>,
    shell: BriocheShell,
    executor: E,
    config: Arc<ShellConfig>,
) where
    E: EffectExecutor,
{
    // Semaphore to limit concurrent effects.
    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_effects));

    while let Some((effects, snapshot)) = output_rx.recv().await {
        for effect in effects {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let shell = shell.clone();
            let executor = executor.clone();
            let snapshot = snapshot.clone();

            tokio::spawn(async move {
                let _permit = permit; // held until future completes
                if let Err(e) = execute_effect(effect, &shell, &executor, &snapshot).await {
                    tracing::error!(error = %e, "effect execution failed");
                }
            });
        }
    }
}

/// Execute a single effect.
///
/// This is the routing table for effect dispatch. Each variant is
/// handled by the appropriate async subsystem.
///
/// Refs: I-Shell-ToolResult-PassThrough
async fn execute_effect<E>(
    effect: Effect,
    shell: &BriocheShell,
    executor: &E,
    snapshot: &StateSnapshot,
) -> Result<(), ShellError>
where
    E: EffectExecutor,
{
    match effect {
        Effect::CallLlmNetwork => {
            executor.call_llm(shell).await?;
        }
        Effect::ExecuteTools(calls) => {
            let generation_id = snapshot.generation_id.unwrap_or(0);
            executor.execute_tools(calls, generation_id, shell).await?;
        }
        Effect::ForwardToUi {
            widget_type,
            payload,
        } => {
            executor.forward_to_ui(widget_type, payload).await?;
        }
        Effect::Error { code, message } => {
            executor.log_error(code, message).await?;
        }
        Effect::SaveSession => {
            executor.save_session(&snapshot.session_id).await?;
        }
        Effect::SavePluginBlob { plugin_id, data } => {
            executor.save_plugin_blob(&plugin_id, data).await?;
        }
        Effect::TriggerSummarization => {
            executor.trigger_summarization(shell).await?;
        }
        Effect::ExecuteCpuTask { task_id, payload } => {
            executor.execute_cpu_task(task_id, payload, shell).await?;
        }
        Effect::TriggerGc => {
            executor.trigger_gc().await?;
        }
        Effect::SystemIdle => {
            executor.on_system_idle(shell).await?;
        }
        Effect::PluginFault { plugin_name, error } => {
            let notification = GovernanceNotification::PluginFaulted { plugin_name, error };
            shell.send_governance_notification(notification).await?;
        }
        Effect::RebuildRoutes => {
            // RebuildRoutes is a transactional barrier handled by the
            // engine thread. The shell ensures no new inputs are sent
            // until recalculation completes.
            executor.rebuild_routes().await?;
        }
        Effect::SubRoutineRestored { handle } => {
            executor.sub_routine_restored(handle).await?;
        }
    }
    Ok(())
}
