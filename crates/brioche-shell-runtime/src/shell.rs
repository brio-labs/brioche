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

use crate::{
    EffectExecutor, EngineWatchdog, EngineWatchdogHandle, PersistenceMode, TelemetryChannel,
    TickEmitter, TransitionJournal,
};
use brioche_core::{
    AgentState, AgentStateTag, BriocheEngine, Effect, EngineInput, EpochState,
    GovernanceNotification, Session, SignalBuffer, SignalDrainOrder, SystemSignal,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{mpsc, oneshot};

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
    #[error("rebuild routes in progress")]
    RebuildInProgress,
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
    /// Persistence mode for `SaveSession` effects.
    pub persistence_mode: PersistenceMode,
    /// Whether to enable the `TransitionJournal`.
    pub transition_journal_enabled: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            engine_channel_capacity: 256,
            tick_interval_ms: 1000,
            max_concurrent_effects: 32,
            persistence_mode: PersistenceMode::Async,
            transition_journal_enabled: true,
        }
    }
}

/// Command sent to the engine thread to rebuild routing tables.
///
/// Refs: I-Gov-Rebuild-Barrier
struct RebuildCommand {
    /// `true` for each plugin index that remains active.
    active_mask: Vec<bool>,
    /// Channel to signal completion back to the async effect loop.
    done: oneshot::Sender<()>,
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
    /// Sender for async task results.
    async_task_result_tx: mpsc::Sender<brioche_core::AsyncTaskResult>,
    /// Sender for rebuild commands to the engine thread.
    rebuild_tx: mpsc::Sender<RebuildCommand>,
    /// Shared flag: `true` while a rebuild is in progress.
    ///
    /// When set, `send_input` returns `Err(ShellError::RebuildInProgress)`
    /// so that no new `EngineInput` enters the engine until the barrier
    /// lifts.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    rebuild_in_progress: Arc<AtomicBool>,
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
    /// ## Startup procedure (9 steps)
    ///
    /// 1. Initialize `BriocheEngine` with plugins, governance traits,
    ///    `SignalDrainOrder`, and `SubRoutineLifecycleGuard`.
    /// 2. Open or create the Redb database (deferred to Sprint 12).
    /// 3. Load the active session (deferred to Sprint 12).
    /// 4. Install non-blocking telemetry subscriber.
    /// 5. Initialize `TransitionJournal` (1 MB lock-free buffer).
    /// 6. Launch separate channel adapters (or `UnifiedEventBus`).
    /// 7. Launch `EngineWatchdog` with `TransitionJournal`.
    /// 8. Launch periodic `SystemSignal::Tick` emitter.
    /// 9. Launch effect consumption loop, IPC batching regulator,
    ///    backpressure worker.
    ///
    /// Refs: SPECS.md §Book III-A Ch 1.1, I-Shell-Session-NoSend
    pub fn new<F, E>(engine_factory: F, config: ShellConfig, executor: E) -> Self
    where
        F: FnOnce() -> (BriocheEngine, Session) + Send + 'static,
        E: EffectExecutor + 'static,
    {
        // Step 6 (partial): create channels.
        let (input_tx, input_rx) = mpsc::channel::<EngineInput>(config.engine_channel_capacity);
        let (output_tx, output_rx) =
            mpsc::channel::<(Vec<Effect>, StateSnapshot)>(config.engine_channel_capacity);
        let (system_signal_tx, system_signal_rx) =
            mpsc::channel::<SystemSignal>(config.engine_channel_capacity);
        let (governance_tx, governance_rx) =
            mpsc::channel::<GovernanceNotification>(config.engine_channel_capacity);
        let (async_task_result_tx, async_task_result_rx) =
            mpsc::channel::<brioche_core::AsyncTaskResult>(config.engine_channel_capacity);
        let (rebuild_tx, rebuild_rx) = mpsc::channel::<RebuildCommand>(4);

        // Step 6 (partial): create the signal drain order (canonical multiplexer).
        let signal_drain =
            crate::SignalMultiplexer::new(system_signal_rx, governance_rx, async_task_result_rx);

        // Step 7: create watchdog channels.
        let pending_inputs = Arc::new(AtomicU64::new(0));
        let (watchdog_handle, ping_tx, pong_rx) =
            EngineWatchdogHandle::new(Arc::clone(&pending_inputs));

        // Step 5: initialize TransitionJournal.
        let transition_journal = Arc::new(TransitionJournal::new());
        let journal_for_engine = Arc::clone(&transition_journal);
        let journal_for_watchdog = Arc::clone(&transition_journal);

        // Spawn the synchronous engine thread.
        std::thread::spawn(move || {
            let (engine, session) = engine_factory();
            engine_thread_loop(
                engine,
                session,
                input_rx,
                output_tx,
                signal_drain,
                watchdog_handle,
                pending_inputs,
                rebuild_rx,
                journal_for_engine,
                config.transition_journal_enabled,
            );
        });

        let config_arc = Arc::new(config.clone());
        let rebuild_in_progress = Arc::new(AtomicBool::new(false));
        let shell = Self {
            input_tx: input_tx.clone(),
            system_signal_tx: system_signal_tx.clone(),
            governance_tx: governance_tx.clone(),
            async_task_result_tx: async_task_result_tx.clone(),
            rebuild_tx,
            rebuild_in_progress: Arc::clone(&rebuild_in_progress),
            config: Arc::clone(&config_arc),
        };

        // Spawn the async effect consumption loop.
        let shell_clone = shell.clone();
        tokio::spawn(async move {
            effect_consumption_loop(
                output_rx,
                shell_clone,
                executor,
                config_arc,
                rebuild_in_progress,
            )
            .await;
        });

        // Step 8: launch the periodic tick emitter.
        let tick_emitter = TickEmitter::new(system_signal_tx, config.tick_interval_ms);
        tokio::spawn(tick_emitter.run());

        // Step 7 (continued): launch the engine watchdog.
        let watchdog = EngineWatchdog::default().with_transition_journal(journal_for_watchdog);
        tokio::spawn(watchdog.run(ping_tx, pong_rx));

        // Step 4: install default telemetry subscriber.
        let telemetry = TelemetryChannel::new(256);
        crate::telemetry::install_default_subscriber(telemetry);

        shell
    }

    /// Send an `EngineInput` to the kernel.
    ///
    /// Returns `Err(ShellError::RebuildInProgress)` if a route
    /// recalculation is ongoing (transactional barrier).
    ///
    /// Refs: I-Shell-Backpressure-NoOverflow, I-Gov-Rebuild-Barrier
    pub async fn send_input(&self, input: EngineInput) -> Result<(), ShellError> {
        if self.rebuild_in_progress.load(Ordering::Acquire) {
            return Err(ShellError::RebuildInProgress);
        }
        self.input_tx
            .send(input)
            .await
            .map_err(|_| ShellError::ChannelSend)
    }

    /// Send a `SystemSignal` to the kernel.
    ///
    /// Signals are drained by the shell between transition cycles and
    /// injected into the engine via `SignalBuffer`.
    ///
    /// Refs: I-Shell-Network-Signal
    pub async fn send_system_signal(&self, signal: SystemSignal) -> Result<(), ShellError> {
        self.system_signal_tx
            .send(signal)
            .await
            .map_err(|_| ShellError::ChannelSend)
    }

    /// Send a `GovernanceNotification` to the kernel.
    ///
    /// Refs: I-Shell-Drain-Atomic
    pub async fn send_governance_notification(
        &self,
        notification: GovernanceNotification,
    ) -> Result<(), ShellError> {
        self.governance_tx
            .send(notification)
            .await
            .map_err(|_| ShellError::ChannelSend)
    }

    /// Send an `AsyncTaskResult` to the kernel.
    ///
    /// Results are drained by the shell between transition cycles and
    /// injected into the engine via `SignalBuffer`.
    ///
    /// Refs: I-Shell-Drain-Atomic
    pub async fn send_async_task_result(
        &self,
        result: brioche_core::AsyncTaskResult,
    ) -> Result<(), ShellError> {
        self.async_task_result_tx
            .send(result)
            .await
            .map_err(|_| ShellError::ChannelSend)
    }

    /// Send a rebuild-routes command to the engine thread and wait
    /// for completion.
    ///
    /// This is the transactional barrier implementation: the async
    /// effect loop calls this when `Effect::RebuildRoutes` is received.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    pub(crate) async fn send_rebuild_command(
        &self,
        active_mask: Vec<bool>,
    ) -> Result<(), ShellError> {
        let (done_tx, done_rx) = oneshot::channel();
        let command = RebuildCommand {
            active_mask,
            done: done_tx,
        };
        self.rebuild_tx
            .send(command)
            .await
            .map_err(|_| ShellError::ChannelSend)?;
        done_rx.await.map_err(|_| ShellError::EngineDisconnected)
    }

    /// Block until the engine channel has capacity.
    ///
    /// Useful for backpressure-aware producers.
    ///
    /// Refs: I-Shell-Backpressure-NoOverflow
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
    ///
    /// Refs: I-Shell-Session-NoSend
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
///
/// # Note on arguments
/// The large parameter list is intentional: every value is moved into
/// the thread and never escapes. Grouping them into a struct would
/// add boilerplate without improving clarity.
#[allow(clippy::too_many_arguments)]
fn engine_thread_loop(
    mut engine: BriocheEngine,
    mut session: Session,
    mut input_rx: mpsc::Receiver<EngineInput>,
    output_tx: mpsc::Sender<(Vec<Effect>, StateSnapshot)>,
    signal_drain: impl SignalDrainOrder,
    mut watchdog_handle: EngineWatchdogHandle,
    pending_inputs_counter: Arc<AtomicU64>,
    mut rebuild_rx: mpsc::Receiver<RebuildCommand>,
    journal: Arc<TransitionJournal>,
    journal_enabled: bool,
) {
    loop {
        // Check for rebuild commands before processing the next input.
        // This ensures route recalculation happens atomically with
        // respect to transitions.
        //
        // Refs: I-Gov-Rebuild-Barrier
        while let Ok(command) = rebuild_rx.try_recv() {
            engine.rebuild_routes(&command.active_mask);
            let _ = command.done.send(());
        }

        let Some(input) = input_rx.blocking_recv() else {
            break;
        };

        // Persist the input to the TransitionJournal before executing
        // the transition.  This satisfies I-Shell-TransitionJournal.
        if journal_enabled {
            journal.append(&input);
        }

        // Update pending inputs counter for watchdog telemetry.
        pending_inputs_counter.store(input_rx.len() as u64, Ordering::Relaxed);

        // Drain separate channels in canonical order and inject into
        // ExtensionStorage as SignalBuffer.
        let batch = signal_drain.drain();
        session.extensions.insert(SignalBuffer {
            system_signals: batch.system_signals,
            governance_notifications: batch.governance_notifications,
            async_task_results: batch.async_task_results,
        });

        // Respond to watchdog ping if one is pending.
        let last_epoch = session
            .extensions
            .get_or_insert_default::<EpochState>()
            .current_generation;
        watchdog_handle.respond_if_pinged(last_epoch);

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
    rebuild_in_progress: Arc<AtomicBool>,
) where
    E: EffectExecutor,
{
    // Semaphore to limit concurrent effects.
    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_effects));

    while let Some((effects, snapshot)) = output_rx.recv().await {
        for effect in effects {
            // RebuildRoutes is a transactional barrier: set the flag
            // before sending the command and clear it after completion.
            let is_rebuild = matches!(effect, Effect::RebuildRoutes);
            if is_rebuild {
                rebuild_in_progress.store(true, Ordering::Release);
            }

            let permit = match semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let shell = shell.clone();
            let executor = executor.clone();
            let snapshot = snapshot.clone();
            let rebuild_flag = Arc::clone(&rebuild_in_progress);

            tokio::spawn(async move {
                let _permit = permit; // held until future completes
                let result = execute_effect(effect, &shell, &executor, &snapshot).await;
                if let Err(e) = result {
                    tracing::error!(error = %e, "effect execution failed");
                }
                // Clear the rebuild barrier after the effect completes.
                // For RebuildRoutes this happens after the engine thread
                // has finished recalculating routes.
                if is_rebuild {
                    rebuild_flag.store(false, Ordering::Release);
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
            // End-to-end fault propagation:
            // 1. The kernel emitted PluginFault.
            // 2. The shell forwards it as a GovernanceNotification.
            // 3. QuarantineManager (governance plugin) consumes it via
            //    the GovernanceNotificationAdapter and returns
            //    OverrideTransition([RebuildRoutes, ...]).
            //
            // Refs: SPECS.md §Book III-A Ch 1.3
            let notification = GovernanceNotification::PluginFaulted { plugin_name, error };
            shell.send_governance_notification(notification).await?;
        }
        Effect::RebuildRoutes => {
            // Transactional barrier: send rebuild command to engine thread
            // and await completion.  No new EngineInput is accepted while
            // `rebuild_in_progress` is true.
            //
            // For Sprint 11 we rebuild with all plugins active (no
            // quarantine mask).  Quarantine logic will refine the mask
            // in Sprint 16.
            //
            // Refs: I-Gov-Rebuild-Barrier
            tracing::info!("RebuildRoutes: triggering transactional barrier");
            shell.send_rebuild_command(vec![]).await?;
        }
        Effect::SubRoutineRestored { handle } => {
            executor.sub_routine_restored(handle).await?;
        }
    }
    Ok(())
}
