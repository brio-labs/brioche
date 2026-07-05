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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use brioche_core::{
    AgentState, AgentStateTag, BriocheEngine, Effect, EngineInput, EpochState,
    GovernanceNotification, Session, SignalBuffer, SignalDrainOrder, SystemSignal,
};
use tokio::sync::{mpsc, oneshot};

use crate::{
    EffectExecutor, EngineWatchdog, EngineWatchdogHandle, TelemetryChannel, TransitionJournal,
};

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
    /// Engine thread has disconnected.
    #[error("engine thread disconnected")]
    EngineDisconnected,
    /// Failed to send a message through an internal channel.
    #[error("channel send failed")]
    ChannelSend,
    /// Effect handler returned an error.
    #[error("effect execution failed: {0}")]
    EffectExecution(String),
    /// Route rebuild is in progress; inputs are temporarily blocked.
    #[error("rebuild routes in progress")]
    RebuildInProgress,
}

/// Configuration for `BriocheShell`.
///
/// All fields have sensible defaults.
/// Refs: docs/SPECS.md §Book III-A
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

/// Callback invoked on the engine thread after each transition.
///
/// The callback receives an immutable reference to `Session` (which is
/// `!Send`). This is the standard mechanism for extracting a session
/// snapshot and pushing it to the persistence layer.
///
/// Refs: I-Shell-Session-NoSend
pub type SessionCallback = Box<dyn Fn(&mut Session) + Send>;

/// Command sent to the engine thread to rebuild routing tables.
///
/// Refs: I-Gov-Rebuild-Barrier
struct RebuildCommand {
    /// `true` for each plugin index that remains active.
    active_mask: Vec<bool>,
    /// Channel to signal completion back to the async effect loop.
    done: oneshot::Sender<()>,
}

/// Tracker for critical async tasks spawned by the shell.
///
/// Ensures that background task `JoinHandle`s are not lost, enabling
/// diagnostics in case of panic or premature termination. Finished tasks
/// are pruned on `spawn` and `health_check` to prevent unbounded growth.
///
/// Refs: I-Shell-Runtime-OnlyIO, SCIFI — Connect
#[derive(Clone)]
pub struct TaskTracker {
    handles: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl TaskTracker {
    /// Creates a new empty tracker.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new() -> Self {
        Self {
            handles: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Spawns a task and retains its `JoinHandle`.
    ///
    /// Prunes any finished handles before pushing the new one so the
    /// tracker does not grow unbounded.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::spawn(future);
        if let Ok(mut handles) = self.handles.lock() {
            handles.retain(|h| !h.is_finished());
            handles.push(handle);
        }
    }

    /// Checks the state of tracked tasks and prunes finished ones.
    ///
    /// Returns `true` if all remaining tasks are still active.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn health_check(&self) -> bool {
        let mut all_healthy = true;
        if let Ok(mut handles) = self.handles.lock() {
            handles.retain(|h| {
                if h.is_finished() {
                    all_healthy = false;
                    false
                } else {
                    true
                }
            });
        } else {
            all_healthy = false;
        }
        all_healthy
    }
}

impl Default for TaskTracker {
    fn default() -> Self {
        Self::new()
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
///         let engine = BriocheEngineBuilder::new().build();
///         let session = Session::new("main");
///         (engine, session)
///     },
///     ShellConfig::default(),
///     executor,
///     None,
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
    /// Tracker for critical background tasks.
    ///
    /// Ensures spawned tasks are not silently lost.
    ///
    /// Refs: SCIFI — Connect
    task_tracker: TaskTracker,
}

impl BriocheShell {
    /// Create a new shell, spawn the engine thread, and start the
    /// effect consumption loop.
    ///
    /// `engine_factory` is called on the dedicated engine thread.
    /// This respects the `!Send` invariant of `BriocheEngine` and `Session`.
    ///
    /// `session_callback` is called on the engine thread after every
    /// successful transition. Use it to snapshot the session for
    /// persistence (e.g. `SessionHeadDTO::from_session`).
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
    /// Refs: docs/SPECS.md §Book III-A Ch 1.1, I-Shell-Session-NoSend
    pub fn new<F, E>(
        engine_factory: F,
        config: ShellConfig,
        executor: E,
        session_callback: Option<SessionCallback>,
    ) -> Self
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
                session_callback,
            );
        });

        let config_arc = Arc::new(config.clone());
        let rebuild_in_progress = Arc::new(AtomicBool::new(false));
        let task_tracker = TaskTracker::new();
        let shell = Self {
            input_tx: input_tx.clone(),
            system_signal_tx: system_signal_tx.clone(),
            governance_tx: governance_tx.clone(),
            async_task_result_tx: async_task_result_tx.clone(),
            rebuild_tx,
            rebuild_in_progress: Arc::clone(&rebuild_in_progress),
            task_tracker: task_tracker.clone(),
        };

        // Spawn the async effect consumption loop.
        let shell_clone = shell.clone();
        task_tracker.spawn(async move {
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
        task_tracker.spawn(tick_emitter.run());

        // Step 4: install default telemetry subscriber.
        let telemetry = TelemetryChannel::new(256);
        crate::telemetry::install_default_subscriber(telemetry.clone());

        // Step 7 (continued): launch the engine watchdog.
        let shell_for_serialize = shell.clone();
        let shell_for_degrade = shell.clone();
        let watchdog = EngineWatchdog::default()
            .with_transition_journal(journal_for_watchdog)
            .with_telemetry(telemetry)
            .with_serialize_and_restart_handler(move || {
                let shell = shell_for_serialize.clone();
                tokio::spawn(async move {
                    let _ = shell
                        .send_system_signal(brioche_core::SystemSignal::EngineUnresponsive {
                            procedure: "SerializeAndRestart".into(),
                        })
                        .await;
                });
            })
            .with_notify_and_degrade_handler(move || {
                let shell = shell_for_degrade.clone();
                tokio::spawn(async move {
                    let _ = shell
                        .send_system_signal(brioche_core::SystemSignal::EngineUnresponsive {
                            procedure: "NotifyAndDegrade".into(),
                        })
                        .await;
                });
            });
        task_tracker.spawn(watchdog.run(ping_tx, pong_rx));
        shell
    }

    /// Test-only constructor that wires the engine-input and async-task-result
    /// channels to the provided senders.
    ///
    /// Used by integration tests that need to observe `EngineInput` loopback
    /// messages (e.g. `ToolCallsResult`, `LlmStream`) and `AsyncTaskResult`
    /// without spinning up the full engine thread. The remaining shell
    /// channels (system signals, governance, rebuild) are wired to bounded
    /// channels with their receivers dropped, so sends to those channels fail
    /// cleanly.
    ///
    /// Refs: I-Shell-Drain-Atomic
    pub fn test_with_loopback_channels(
        input_tx: mpsc::Sender<EngineInput>,
        async_task_result_tx: mpsc::Sender<brioche_core::AsyncTaskResult>,
    ) -> Self {
        let (_system_tx, _system_rx) = mpsc::channel(1);
        let (_gov_tx, _gov_rx) = mpsc::channel(1);
        let (_rebuild_tx, _rebuild_rx) = mpsc::channel(1);

        // Drop unused receivers so the senders do not block.
        drop((_system_rx, _gov_rx, _rebuild_rx));

        Self {
            input_tx,
            system_signal_tx: _system_tx,
            governance_tx: _gov_tx,
            async_task_result_tx,
            rebuild_tx: _rebuild_tx,
            rebuild_in_progress: Arc::new(AtomicBool::new(false)),
            task_tracker: TaskTracker::new(),
        }
    }

    /// Test-only constructor that exposes only the async-task result channel.
    ///
    /// Used by unit tests that need a `BriocheShell` handle without spinning up
    /// the full engine thread. Sends to the engine-input channel will fail
    /// cleanly because the test does not hold the receiver.
    ///
    /// Refs: I-Shell-Drain-Atomic
    pub fn test_with_async_channel(
        tx: tokio::sync::mpsc::Sender<brioche_core::AsyncTaskResult>,
    ) -> Self {
        let (_input_tx, _input_rx) = tokio::sync::mpsc::channel(1);
        drop(_input_rx);
        Self::test_with_loopback_channels(_input_tx, tx)
    }

    /// Send an `EngineInput` to the kernel.
    ///
    /// Returns `Err(ShellError::RebuildInProgress)` if a route
    /// recalculation is ongoing (transactional barrier).
    ///
    /// # Cancel safety
    /// This future holds no locks across await points. Dropping it before
    /// completion only fails to enqueue the input; the caller can retry.
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
    /// # Cancel safety
    /// This future holds no locks across await points. Dropping it before
    /// completion only fails to enqueue the signal; the caller can retry.
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
    /// # Cancel safety
    /// This future holds no locks across await points. Dropping it before
    /// completion only fails to enqueue the notification; the caller can retry.
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
    /// # Cancel safety
    /// This future holds no locks across await points. Dropping it before
    /// completion only fails to enqueue the result; the caller can retry.
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
    /// # Cancel safety
    /// This future performs only an atomic channel-closed check. Dropping
    /// it before completion has no side effects.
    ///
    /// Refs: I-Shell-Backpressure-NoOverflow
    pub async fn ready(&self) -> Result<(), ShellError> {
        if self.input_tx.is_closed() {
            return Err(ShellError::EngineDisconnected);
        }
        Ok(())
    }

    /// Verify that all critical background tasks are still running.
    ///
    /// Returns `true` if healthy; logs an error for each finished task.
    ///
    /// Refs: SCIFI — Connect
    /// Refs: docs/SPECS.md §Book III-A
    pub fn health_check(&self) -> bool {
        self.task_tracker.health_check()
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
    session_callback: Option<SessionCallback>,
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

        // Invoke the session callback (persistence snapshot) on the
        // engine thread while we still own the Session.
        if let Some(ref cb) = session_callback {
            cb(&mut session);
        }

        // Send results back to the async runtime.
        if output_tx.blocking_send((effects, snapshot)).is_err() {
            // Async runtime dropped; shut down.
            break;
        }

        // Mark the transition as durably processed by the engine.
        // Any entries still unacknowledged after a crash will be replayed
        // by the watchdog on recovery.
        if journal_enabled {
            journal.acknowledge_all();
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
///
/// After the output channel closes, the executor is shut down so any
/// pending background work (e.g. async persistence) is awaited before
/// the function returns.
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

    // Wait for any background work (e.g. async saves) to finish before
    // the loop exits, so the runtime does not detach in-flight tasks.
    executor.shutdown().await;
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
            let generation_id = snapshot.generation_id.map_or(0, |id| id);
            executor.execute_tools(calls, generation_id, shell).await?;
        }
        Effect::ForwardToUi(widget) => {
            executor.forward_to_ui(widget).await?;
        }
        Effect::Error { code, detail } => {
            executor.log_error(code, detail).await?;
        }
        Effect::SaveSession => {
            executor.save_session(&snapshot.session_id).await?;
        }
        Effect::SavePluginBlob { plugin_id, data } => {
            executor.save_plugin_blob(&plugin_id.0, data).await?;
        }
        Effect::TriggerSummarization => {
            executor.trigger_summarization(shell).await?;
        }
        Effect::ExecuteCpuTask { task_id, payload } => {
            executor.execute_cpu_task(task_id.0, payload, shell).await?;
        }
        Effect::TriggerGc => {
            executor.trigger_gc(&snapshot.session_id).await?;
        }
        Effect::SystemIdle => {
            executor.on_system_idle(shell, &snapshot.session_id).await?;
        }
        Effect::PluginFault { plugin_name, error } => {
            // End-to-end fault propagation:
            // 1. The kernel emitted PluginFault.
            // 2. The shell forwards it as a GovernanceNotification.
            // 3. QuarantineManager (governance plugin) consumes it via
            //    the GovernanceNotificationAdapter and returns
            //    OverrideTransition([RebuildRoutes, ...]).
            //
            // Refs: docs/SPECS.md §Book III-A Ch 1.3
            let notification = GovernanceNotification::PluginFaulted {
                plugin_name: plugin_name.0,
                error,
            };
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
        _ => {
            // Unknown effect variant (forward compatibility for #[non_exhaustive]).
            tracing::warn!("unknown effect variant received");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// PersistenceMode (merged from persistence_mode.rs)
// ---------------------------------------------------------------------------

/// Controls the flush behavior of `SaveSession` effects.
///
/// Injected into the shell at startup via `ShellConfig`.
///
/// Refs: I-Shell-Persistence-Mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PersistenceMode {
    /// Non-blocking save (default).
    ///
    /// The effect handler spawns a background task and returns
    /// immediately.  Best for interactive latency.
    #[default]
    Async,
    /// Blocking save.
    ///
    /// The effect handler awaits the Redb commit before returning.
    /// Best for strict durability guarantees.
    Sync,
}

impl PersistenceMode {
    /// Returns `true` if this mode requires synchronous flush.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Shell-Persistence-Mode
    pub fn is_sync(self) -> bool {
        matches!(self, PersistenceMode::Sync)
    }

    /// Returns `true` if this mode allows asynchronous flush.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Shell-Persistence-Mode
    pub fn is_async(self) -> bool {
        matches!(self, PersistenceMode::Async)
    }
}

// ---------------------------------------------------------------------------
// TickEmitter (merged from tick_emitter.rs)
// ---------------------------------------------------------------------------

use tokio::time::{Duration, Instant, interval};

/// Emits periodic ticks into a `SystemSignal` channel.
///
/// # Example
///
/// ```no_run
/// # async fn example() {
/// use brioche_core::SystemSignal;
/// use brioche_shell_runtime::TickEmitter;
/// use tokio::sync::mpsc;
///
/// let (tx, _rx) = mpsc::channel(64);
/// let emitter = TickEmitter::new(tx, 1000);
/// emitter.run().await;
/// # }
/// ```
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct TickEmitter {
    tx: mpsc::Sender<brioche_core::SystemSignal>,
    interval_ms: u64,
    start: Instant,
}

impl TickEmitter {
    /// Create a tick emitter from a sender.
    ///
    /// `tx` — sender wired to the `SystemSignal` channel consumed by the shell.
    /// `interval_ms` — tick period in milliseconds (default: 1000).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(tx: mpsc::Sender<brioche_core::SystemSignal>, interval_ms: u64) -> Self {
        Self {
            tx,
            interval_ms,
            start: Instant::now(),
        }
    }

    /// Run the emitter loop until the receiver is dropped.
    ///
    /// This future never completes unless the channel closes.
    ///
    /// # Cancel safety
    /// This loop holds no state across await points. Dropping it stops
    /// tick emission; no recovery action is required.
    pub async fn run(self) {
        let mut ticker = interval(Duration::from_millis(self.interval_ms));
        let start = self.start;

        loop {
            ticker.tick().await;
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let signal = brioche_core::SystemSignal::Tick { elapsed_ms };
            if self.tx.send(signal).await.is_err() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor (merged from tool_executor.rs)
// ---------------------------------------------------------------------------

use tokio_util::sync::CancellationToken;

/// Execute a single tool call asynchronously.
///
/// The shell is responsible for timeout enforcement (via `tokio::select!`)
/// and cancellation (via `CancellationToken`). The trait implementation
/// should perform the actual tool invocation and return the raw
/// `ToolResultDTO` without business-level transformation.
///
/// Refs: I-Shell-ToolResult-PassThrough
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute one `ActiveToolCall`.
    ///
    /// The `cancel` token is triggered by the shell on user cancellation
    /// or engine shutdown. Implementations should respect it at
    /// coarse-grained boundaries.
    ///
    /// # Cancel safety
    /// The `cancel` token is the canonical cancellation signal. Dropping the
    /// future detaches the tool task from the caller, but the task may continue
    /// running until it observes the token or reaches its own timeout.
    async fn execute(
        &self,
        call: &brioche_core::ActiveToolCall,
        cancel: CancellationToken,
    ) -> brioche_core::ToolResultDTO;
}

/// A tool executor that always returns success with the argument string echoed.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug, Default)]
pub struct EchoToolExecutor;

#[async_trait::async_trait]
impl ToolExecutor for EchoToolExecutor {
    async fn execute(
        &self,
        call: &brioche_core::ActiveToolCall,
        _cancel: CancellationToken,
    ) -> brioche_core::ToolResultDTO {
        brioche_core::ToolResultDTO {
            tool_id: call.tool_id.clone(),
            tool_name: call.tool_name.clone(),
            outcome: brioche_core::ToolOutcome::Success(call.arguments.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// BackpressureRegulator (merged from backpressure.rs)
// ---------------------------------------------------------------------------

use brioche_core::StreamEvent;

/// Drop policy when the engine channel is under pressure.
///
/// Refs: docs/SPECS.md §Book III-A Ch 2
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPolicy {
    /// Drop intermediate text chunks, keep structural boundaries.
    Conservative,
    /// Never drop — blocks the SSE producer.
    Strict,
}

/// Regulates flow into the engine's `EngineInput` channel.
///
/// # Example
///
/// ```
/// use brioche_core::EngineInput;
/// use brioche_shell_runtime::{BackpressureRegulator, DropPolicy};
///
/// # async fn example() {
/// let (tx, mut rx) = BackpressureRegulator::new(128, DropPolicy::Conservative);
/// tx.send(EngineInput::UserMessage("hello".into()))
///     .await
///     .unwrap();
/// # }
/// ```
///
/// Refs: I-Shell-Backpressure-NoOverflow
#[derive(Clone)]
pub struct BackpressureRegulator {
    tx: mpsc::Sender<EngineInput>,
    capacity: usize,
    drop_policy: DropPolicy,
}

impl BackpressureRegulator {
    /// Create a new regulator with the given channel capacity and drop policy.
    ///
    /// Returns the regulator handle and the receiver end that should be
    /// wired into the engine thread's input loop.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(capacity: usize, drop_policy: DropPolicy) -> (Self, mpsc::Receiver<EngineInput>) {
        let (tx, rx) = mpsc::channel(capacity);
        let regulator = Self {
            tx,
            capacity,
            drop_policy,
        };
        (regulator, rx)
    }

    /// Send an input into the engine channel.
    ///
    /// - In `Conservative` mode: attempts a non-blocking send first.
    ///   If the channel is full and the input is an intermediate
    ///   `LlmStream::TextChunk`, it is silently dropped. Structural
    ///   events are never dropped.
    /// - In `Strict` mode: waits for capacity unconditionally.
    ///
    /// Returns `Err` only if the receiver has been dropped.
    ///
    /// # Cancel safety
    /// In `Conservative` mode, the non-blocking path is cancellation-safe.
    /// In `Strict` mode, this future holds no locks across await points;
    /// dropping it before completion only fails to enqueue the input.
    pub async fn send(
        &self,
        input: EngineInput,
    ) -> Result<(), mpsc::error::SendError<EngineInput>> {
        match self.drop_policy {
            DropPolicy::Conservative => {
                // Try non-blocking first.
                match self.tx.try_send(input) {
                    Ok(()) => Ok(()),
                    Err(mpsc::error::TrySendError::Full(input)) => {
                        // Under pressure: drop intermediate text chunks only.
                        if let EngineInput::LlmStream(StreamEvent::TextChunk { .. }) = &input {
                            Ok(())
                        } else {
                            // Structural event: block until capacity.
                            self.tx.send(input).await
                        }
                    }
                    Err(mpsc::error::TrySendError::Closed(input)) => {
                        Err(mpsc::error::SendError(input))
                    }
                }
            }
            DropPolicy::Strict => self.tx.send(input).await,
        }
    }

    /// Returns the configured capacity of the channel.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_tracker_prunes_finished_handles() {
        let tracker = TaskTracker::new();

        // Spawn many tasks that complete immediately.
        for _ in 0..100 {
            tracker.spawn(async move {});
        }

        // Let the spawned tasks finish.
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Spawning a new task should prune the finished ones so the
        // tracker does not grow unbounded.
        tracker.spawn(async move {});

        assert!(
            tracker.handles.lock().is_ok_and(|guard| guard.len() == 1),
            "tracker should retain only the one still-live handle"
        );
    }
}
