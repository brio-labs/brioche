//! Effect execution dispatch.
//!
//! The [`EffectExecutor`] trait is the boundary between the kernel's
//! declarative `Effect` enum and the shell's async I/O subsystems.
//!
//! A default implementation [`DefaultEffectExecutor`] is provided; it
//! delegates to pluggable traits (`ToolExecutor`, `LlmClient`, `Persistence`).
//!
//! Refs: I-Shell-Runtime-OnlyIO, I-Shell-ToolResult-PassThrough

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use brioche_core::{
    ActiveToolCall, AsyncTaskResult, ChatMessage, ErrorCode, ErrorDetail, SubRoutineHandle,
    SystemSignal, ToolResultDTO, UiWidget,
};

use crate::{BriocheShell, NetworkRecovery, PersistenceMode, ShellError};

/// Synchronous CPU-bound computation handler.
///
/// Receives the serialized input payload and returns the serialized output.
/// Handlers run on Tokio's `spawn_blocking` thread pool.
///
/// Refs: I-Shell-CpuTask-Dispatch
pub type CpuTaskHandler = Arc<dyn Fn(&[u8]) -> Result<Vec<u8>, ShellError> + Send + Sync + 'static>;

/// Registry of synchronous CPU-bound task handlers keyed by task ID.
///
/// Plugins register handlers for task IDs they emit via
/// `#[brioche_offload_task]`. Unregistered tasks fall back to an identity
/// passthrough so the runtime remains backward-compatible.
///
/// Uses `BTreeMap` for deterministic ordering and to satisfy
/// I-Eco-OrderedCollections.
///
/// Refs: I-Shell-CpuTask-Dispatch, I-Eco-OrderedCollections
#[derive(Clone, Default)]
pub struct CpuTaskRegistry {
    handlers: Arc<RwLock<BTreeMap<String, CpuTaskHandler>>>,
}

impl CpuTaskRegistry {
    /// Creates an empty registry.
    ///
    /// Refs: I-Shell-CpuTask-Dispatch
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for the given task ID.
    ///
    /// The handler receives the serialized input payload and must return the
    /// serialized output. It runs on Tokio's `spawn_blocking` thread pool.
    ///
    /// # Complexity
    /// O(log n) where n is the number of registered handlers.
    ///
    /// # Panics
    /// Never panics. A poisoned lock is recovered with `into_inner`.
    ///
    /// Refs: I-Shell-CpuTask-Dispatch
    pub fn register<F>(&mut self, task_id: impl Into<String>, handler: F)
    where
        F: Fn(&[u8]) -> Result<Vec<u8>, ShellError> + Send + Sync + 'static,
    {
        let mut guard = match self.handlers.write() {
            Ok(g) => g,
            Err(err) => err.into_inner(),
        };
        let _ = guard.insert(task_id.into(), Arc::new(handler));
    }

    /// Execute the handler for `task_id`, falling back to identity.
    ///
    /// # Complexity
    /// O(log n) where n is the number of registered handlers.
    ///
    /// # Panics
    /// Never panics. A poisoned lock is recovered with `into_inner`.
    ///
    /// Refs: I-Shell-CpuTask-Dispatch
    pub fn execute(&self, task_id: &str, payload: &[u8]) -> Result<Vec<u8>, ShellError> {
        let guard = match self.handlers.read() {
            Ok(g) => g,
            Err(err) => err.into_inner(),
        };
        match guard.get(task_id) {
            Some(handler) => handler(payload),
            None => Ok(payload.to_vec()),
        }
    }
}

/// Pluggable persistence boundary.
///
/// Concrete implementations live in `brioche-shell-persistence` (Sprint 12).
/// The default `NoopPersistence` silently discards save requests.
///
/// Refs: I-Persist-SaveSession, I-Persist-PluginBlob
#[async_trait::async_trait]
pub trait Persistence: Send + Sync {
    /// Persist the session head and message delta.
    async fn save_session(&self, session_id: &str) -> Result<(), ShellError>;

    /// Persist a cold plugin blob.
    async fn save_plugin_blob(&self, plugin_id: &str, data: Vec<u8>) -> Result<(), ShellError>;

    /// Run opportunistic garbage collection for the given session.
    ///
    /// Returns the number of stale entries removed. Implementations that
    /// do not support GC may return `Ok(0)`.
    ///
    /// Refs: I-Persist-GC-Interrupt
    async fn gc(&self, _session_id: &str) -> Result<u64, ShellError> {
        Ok(0)
    }
}

/// No-op persistence for testing and headless profiles.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug, Default)]
pub struct NoopPersistence;

#[async_trait::async_trait]
impl Persistence for NoopPersistence {
    async fn save_session(&self, _session_id: &str) -> Result<(), ShellError> {
        Ok(())
    }

    async fn save_plugin_blob(&self, _plugin_id: &str, _data: Vec<u8>) -> Result<(), ShellError> {
        Ok(())
    }
}

/// Async dispatcher for all `Effect` variants emitted by the kernel.
///
/// Implementations are cheaply cloneable (typically `Arc`-wrapped) so
/// they can be moved into spawned tasks.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[async_trait::async_trait]
pub trait EffectExecutor: Clone + Send + Sync + 'static {
    /// Invoke the LLM network and stream fragments back as `EngineInput::LlmStream`.
    ///
    /// # Cancel safety
    /// This future delegates to the LLM client and may await network I/O.
    /// Dropping it before completion leaves the underlying request running
    /// until the provider times out; any chunks already emitted remain in the
    /// shell's input queue.
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), ShellError>;

    /// Execute tools in parallel and send `EngineInput::ToolCallsResult`
    /// back to the kernel.
    ///
    /// `generation_id` is captured from the engine state snapshot so the
    /// shell never reads `Session` directly.
    ///
    /// # Cancel safety
    /// This future spawns one task per tool call and awaits the join handles.
    /// Dropping it before all tasks complete detaches the remaining tasks;
    /// their results may still be delivered to the shell and the tool
    /// processes continue until completion or timeout.
    async fn execute_tools(
        &self,
        calls: Vec<ActiveToolCall>,
        generation_id: u64,
        shell: &BriocheShell,
    ) -> Result<(), ShellError>;

    /// Forward a structured UI widget to the presentation layer.
    ///
    /// # Cancel safety
    /// This future reads `self.ui_forwarder` and performs no await after that.
    /// Dropping it is always safe and leaves no partial state.
    async fn forward_to_ui(&self, widget: UiWidget) -> Result<(), ShellError>;

    /// Log an error effect (telemetry / tracing).
    ///
    /// # Cancel safety
    /// This future performs no await. Dropping it is safe.
    async fn log_error(&self, code: ErrorCode, detail: ErrorDetail) -> Result<(), ShellError>;

    /// Persist session state.
    ///
    /// # Cancel safety
    /// In async persistence mode this future spawns a background task, stores
    /// its handle, and returns immediately; the save continues to completion
    /// or error after drop unless `EffectExecutor::shutdown` is awaited. In
    /// sync mode it awaits `Persistence::save_session`; dropping leaves no
    /// partial state in the executor.
    async fn save_session(&self, session_id: &str) -> Result<(), ShellError>;

    /// Persist a plugin blob.
    ///
    /// # Cancel safety
    /// This future awaits `Persistence::save_plugin_blob`; dropping leaves no
    /// partial state in the executor.
    async fn save_plugin_blob(&self, plugin_id: &str, data: Vec<u8>) -> Result<(), ShellError>;

    /// Trigger a background summarization task.
    ///
    /// # Cancel safety
    /// This future awaits a read lock on the history mirror and then the LLM
    /// summarize call. Dropping before the summarize call returns releases the
    /// lock and does not modify history; dropping during
    /// `send_async_task_result` may leave the summary unreported.
    async fn trigger_summarization(&self, shell: &BriocheShell) -> Result<(), ShellError>;

    /// Execute a CPU-intensive task on the blocking thread pool.
    ///
    /// # Cancel safety
    /// This future awaits a `spawn_blocking` task. Dropping before the task
    /// completes detaches it; the computation continues but its result is
    /// discarded. Dropping after completion but before the result is sent
    /// discards the result.
    async fn execute_cpu_task(
        &self,
        task_id: String,
        payload: Vec<u8>,
        shell: &BriocheShell,
    ) -> Result<(), ShellError>;

    /// Trigger opportunistic garbage collection.
    ///
    /// # Cancel safety
    /// This future awaits persistence garbage collection; dropping leaves no
    /// partial state.
    async fn trigger_gc(&self, session_id: &str) -> Result<(), ShellError>;

    /// Handle `SystemIdle` — may decide to trigger GC.
    ///
    /// # Cancel safety
    /// This future performs no await. Dropping it is safe.
    async fn on_system_idle(
        &self,
        shell: &BriocheShell,
        session_id: &str,
    ) -> Result<(), ShellError>;

    /// Rebuild routing tables (transactional barrier).
    ///
    /// # Cancel safety
    /// This future performs no await. Dropping it is safe.
    async fn rebuild_routes(&self) -> Result<(), ShellError>;

    /// Notify that a sub-routine has been restored.
    ///
    /// # Cancel safety
    /// This future invokes a synchronous callback with no await. Dropping
    /// before the callback runs leaves the subroutine unnotified.
    async fn sub_routine_restored(&self, handle: SubRoutineHandle) -> Result<(), ShellError>;

    /// Gracefully shut down the executor.
    ///
    /// Waits for any background work started by the executor to complete.
    /// Implementations that do not spawn background work may leave this
    /// as a no-op.
    ///
    /// # Cancel safety
    /// This future awaits tracked background tasks. Dropping it before
    /// completion leaves those tasks running.
    async fn shutdown(&self) {}
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

/// Default effect executor that delegates to pluggable subsystems.
///
/// `T` — `ToolExecutor`
/// `L` — `LlmClient`
/// `P` — `Persistence`
/// Refs: docs/SPECS.md §Book III-A
pub struct DefaultEffectExecutor<T, L, P> {
    tool_executor: Arc<T>,
    llm_client: Arc<L>,
    persistence: Arc<P>,
    /// Controls `SaveSession` flush behavior.
    ///
    /// Refs: I-Shell-Persistence-Mode
    persistence_mode: PersistenceMode,
    /// Retry/backoff policy for network calls.
    ///
    /// Refs: I-Shell-Network-Signal
    network_recovery: Option<Arc<dyn NetworkRecovery>>,
    /// Optional callback invoked on every `Effect::ForwardToUi`.
    ///
    /// The kernel emits structured widgets; downstream projection layers
    /// (Tauri, CLI) register this hook to consume them. When `None`, the
    /// effect is silently dropped.
    ///
    /// Refs: I-Shell-Projection-Independent
    ui_forwarder: Option<Arc<dyn Fn(UiWidget) + Send + Sync>>,
    /// Optional callback invoked when a sub-routine finishes restoration.
    ///
    /// Projection layers use this to transition the accordion from
    /// `Loading` to `Loaded`. When `None`, the effect is a no-op.
    ///
    /// Refs: I-UI-NoDirectDOM
    subroutine_restored_callback: Option<Arc<dyn Fn(SubRoutineHandle) + Send + Sync>>,
    /// Shared conversational history mirror.
    ///
    /// Used by context compression to select messages for summarization.
    /// When `None`, summarization falls back to a no-op placeholder.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    history: Option<Arc<tokio::sync::RwLock<Vec<ChatMessage>>>>,
    /// Registry of CPU-bound computation handlers.
    ///
    /// When `None`, CPU tasks fall back to identity passthrough.
    ///
    /// Refs: I-Shell-CpuTask-Dispatch
    cpu_task_registry: Option<Arc<CpuTaskRegistry>>,
    /// Handles for in-flight async `SaveSession` effects.
    ///
    /// Stored so `shutdown` can await their completion before the
    /// runtime exits.
    ///
    /// Refs: I-Shell-Persistence-Mode
    async_save_handles: Arc<std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl<T, L, P> Clone for DefaultEffectExecutor<T, L, P> {
    fn clone(&self) -> Self {
        Self {
            tool_executor: Arc::clone(&self.tool_executor),
            llm_client: Arc::clone(&self.llm_client),
            persistence: Arc::clone(&self.persistence),
            persistence_mode: self.persistence_mode,
            network_recovery: self.network_recovery.as_ref().map(Arc::clone),
            ui_forwarder: self.ui_forwarder.as_ref().map(Arc::clone),
            subroutine_restored_callback: self
                .subroutine_restored_callback
                .as_ref()
                .map(Arc::clone),
            history: self.history.as_ref().map(Arc::clone),
            cpu_task_registry: self.cpu_task_registry.as_ref().map(Arc::clone),
            async_save_handles: Arc::clone(&self.async_save_handles),
        }
    }
}

impl<T, L, P> DefaultEffectExecutor<T, L, P> {
    /// Creates a new effect executor with the given tools, LLM client, and persistence store.
    ///
    /// Refs: I-Shell-EffectExecutor-Construction
    pub fn new(tool_executor: T, llm_client: L, persistence: P) -> Self {
        Self {
            tool_executor: Arc::new(tool_executor),
            llm_client: Arc::new(llm_client),
            persistence: Arc::new(persistence),
            persistence_mode: PersistenceMode::Async,
            network_recovery: None,
            ui_forwarder: None,
            subroutine_restored_callback: None,
            history: None,
            cpu_task_registry: None,
            async_save_handles: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Set the persistence mode.
    ///
    /// Refs: I-Shell-Persistence-Mode
    pub fn with_persistence_mode(mut self, mode: PersistenceMode) -> Self {
        self.persistence_mode = mode;
        self
    }

    /// Set the network recovery policy.
    ///
    /// Refs: I-Shell-Network-Signal
    pub fn with_network_recovery<R: NetworkRecovery + 'static>(mut self, recovery: R) -> Self {
        self.network_recovery = Some(Arc::new(recovery));
        self
    }

    /// Register a callback for `Effect::ForwardToUi`.
    ///
    /// Refs: I-Shell-Projection-Independent
    pub fn with_ui_forwarder<F: Fn(UiWidget) + Send + Sync + 'static>(
        mut self,
        forwarder: F,
    ) -> Self {
        self.ui_forwarder = Some(Arc::new(forwarder));
        self
    }

    /// Register a callback for `Effect::SubRoutineRestored`.
    ///
    /// Refs: I-UI-NoDirectDOM
    pub fn with_subroutine_restored_callback<F: Fn(SubRoutineHandle) + Send + Sync + 'static>(
        mut self,
        callback: F,
    ) -> Self {
        self.subroutine_restored_callback = Some(Arc::new(callback));
        self
    }

    /// Attach the shared history mirror used for context compression.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn with_history(mut self, history: Arc<tokio::sync::RwLock<Vec<ChatMessage>>>) -> Self {
        self.history = Some(history);
        self
    }

    /// Attach a registry of CPU-bound computation handlers.
    ///
    /// When attached, `Effect::ExecuteCpuTask` payloads are dispatched to the
    /// handler registered for the task ID. Unregistered tasks fall back to an
    /// identity passthrough.
    ///
    /// Refs: I-Shell-CpuTask-Dispatch
    pub fn with_cpu_task_registry(mut self, registry: Arc<CpuTaskRegistry>) -> Self {
        self.cpu_task_registry = Some(registry);
        self
    }
}

#[async_trait::async_trait]
impl<T, L, P> EffectExecutor for DefaultEffectExecutor<T, L, P>
where
    T: crate::ToolExecutor + 'static,
    L: crate::LlmClient + 'static,
    P: Persistence + 'static,
{
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), ShellError> {
        // Apply retry/backoff policy at the transport level.
        // The kernel receives only complete LlmStream events or
        // SystemSignal::NetworkUnavailable as a last resort.
        //
        // Refs: I-Shell-Network-Signal
        let mut attempt: u32 = 0;
        loop {
            match self.llm_client.call_llm(shell).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    let error_str = err.to_string();
                    if let Some(ref recovery) = self.network_recovery {
                        match recovery.next_retry(attempt, &error_str) {
                            Some(delay) => {
                                tracing::warn!(
                                    attempt,
                                    delay_ms = delay.as_millis(),
                                    error = %error_str,
                                    "llm call failed, retrying"
                                );
                                tokio::time::sleep(delay).await;
                                attempt += 1;
                            }
                            None => {
                                tracing::error!(
                                    attempts = attempt,
                                    error = %error_str,
                                    "llm call exhausted retries, emitting NetworkUnavailable"
                                );
                                shell
                                    .send_system_signal(SystemSignal::NetworkUnavailable {
                                        reason: error_str,
                                    })
                                    .await?;
                                return Ok(());
                            }
                        }
                    } else {
                        // No recovery policy: immediately emit NetworkUnavailable.
                        shell
                            .send_system_signal(SystemSignal::NetworkUnavailable {
                                reason: error_str.clone(),
                            })
                            .await?;
                        return Ok(());
                    }
                }
            }
        }
    }

    async fn execute_tools(
        &self,
        calls: Vec<ActiveToolCall>,
        generation_id: u64,
        shell: &BriocheShell,
    ) -> Result<(), ShellError> {
        use brioche_core::{EngineInput, ToolOutcome};
        use tokio_util::sync::CancellationToken;

        let mut handles = Vec::with_capacity(calls.len());

        for call in calls {
            let tool_executor = Arc::clone(&self.tool_executor);
            let cancel = CancellationToken::new();
            // ActiveToolCall.timeout_ms is the mechanical source of truth.
            // The kernel's seal() already materializes this from the descriptor
            // with default_tool_timeout_ms as fallback (docs/SPECS.md §Book III-A Ch 1).
            let timeout_ms = call.timeout_ms;

            let handle = tokio::spawn(async move {
                let result = tokio::select! {
                    biased;
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(timeout_ms)) => {
                        ToolResultDTO {
                            tool_id: call.tool_id.clone(),
                            tool_name: call.tool_name.clone(),
                            outcome: ToolOutcome::TimeoutWithPartialData { partial_output: None },
                        }
                    }
                    r = tool_executor.execute(&call, cancel.clone()) => r,
                };
                result
            });
            handles.push(handle);
        }

        let mut results = Vec::with_capacity(handles.len());
        for h in handles {
            match h.await {
                Ok(r) => results.push(r),
                Err(join_err) => {
                    return Err(ShellError::EffectExecution(format!(
                        "tool task panicked: {}",
                        join_err
                    )));
                }
            }
        }

        shell
            .send_input(EngineInput::ToolCallsResult {
                generation_id,
                results,
            })
            .await
    }

    async fn forward_to_ui(&self, widget: UiWidget) -> Result<(), ShellError> {
        if let Some(forwarder) = &self.ui_forwarder {
            forwarder(widget);
        }
        Ok(())
    }

    async fn log_error(&self, code: ErrorCode, detail: ErrorDetail) -> Result<(), ShellError> {
        tracing::error!(?code, %detail, "engine error effect");
        Ok(())
    }

    async fn save_session(&self, session_id: &str) -> Result<(), ShellError> {
        let persistence = Arc::clone(&self.persistence);
        let id = session_id.to_string();
        match self.persistence_mode {
            PersistenceMode::Async => {
                // Non-blocking: spawn the save on a background task and retain
                // the handle so shutdown can await it.
                let handle = tokio::spawn(async move {
                    if let Err(err) = persistence.save_session(&id).await {
                        tracing::error!(error = %err, "async save_session failed");
                    }
                });
                if let Ok(mut handles) = self.async_save_handles.lock() {
                    handles.retain(|h| !h.is_finished());
                    handles.push(handle);
                }
                Ok(())
            }
            PersistenceMode::Sync => {
                // Blocking: await the commit before returning.
                self.persistence.save_session(session_id).await
            }
        }
    }

    async fn save_plugin_blob(&self, plugin_id: &str, data: Vec<u8>) -> Result<(), ShellError> {
        self.persistence.save_plugin_blob(plugin_id, data).await
    }

    async fn trigger_summarization(&self, shell: &BriocheShell) -> Result<(), ShellError> {
        const KEEP_RECENT: usize = 2;

        let messages_to_summarize = match &self.history {
            Some(history) => {
                let guard = history.read().await;
                if guard.len() > KEEP_RECENT {
                    guard[..guard.len() - KEEP_RECENT].to_vec()
                } else {
                    Vec::new()
                }
            }
            None => {
                // No history mirror available: emit a no-op placeholder
                // rather than failing the effect.
                return Ok(());
            }
        };

        if messages_to_summarize.is_empty() {
            return Ok(());
        }

        let watermark = messages_to_summarize.len() as u32;
        let summary = self
            .llm_client
            .summarize(shell, &messages_to_summarize)
            .await?;

        shell
            .send_async_task_result(AsyncTaskResult::SummarizationDone { summary, watermark })
            .await
    }

    async fn execute_cpu_task(
        &self,
        task_id: String,
        payload: Vec<u8>,
        shell: &BriocheShell,
    ) -> Result<(), ShellError> {
        let registry = self.cpu_task_registry.clone();
        let task_id_for_handler = task_id.clone();
        let result = tokio::task::spawn_blocking(move || match registry {
            Some(r) => r.execute(&task_id_for_handler, &payload),
            None => Ok(payload),
        })
        .await
        .map_err(|e| ShellError::EffectExecution(format!("cpu task panicked: {}", e)))??;

        shell
            .send_async_task_result(AsyncTaskResult::CpuTaskDone { task_id, result })
            .await
    }

    async fn trigger_gc(&self, session_id: &str) -> Result<(), ShellError> {
        let removed = self.persistence.gc(session_id).await?;
        if removed > 0 {
            tracing::info!(session_id, removed, "GC completed");
        }
        Ok(())
    }

    async fn on_system_idle(
        &self,
        _shell: &BriocheShell,
        _session_id: &str,
    ) -> Result<(), ShellError> {
        // GC is now requested by the `GcPolicy` plugin via `Effect::TriggerGc`.
        // This hook is intentionally a no-op so the runtime can still observe
        // idle transitions without coupling to a static config flag.
        Ok(())
    }

    async fn rebuild_routes(&self) -> Result<(), ShellError> {
        // RebuildRoutes is handled by the shell's transactional barrier
        // in `execute_effect`.  The effect executor itself performs no
        // additional work.
        //
        // Refs: I-Gov-Rebuild-Barrier
        Ok(())
    }

    async fn sub_routine_restored(&self, handle: SubRoutineHandle) -> Result<(), ShellError> {
        if let Some(callback) = &self.subroutine_restored_callback {
            callback(handle);
        }
        Ok(())
    }

    /// Wait for any in-flight async `SaveSession` effects to complete.
    ///
    /// Errors are ignored; the runtime is terminating.
    ///
    /// Refs: I-Shell-Persistence-Mode
    async fn shutdown(&self) {
        // Drain the shared handle list so each handle is awaited exactly once
        // even though every clone of the executor shares the same list.
        let handles = {
            if let Ok(mut guard) = self.async_save_handles.lock() {
                std::mem::take(&mut *guard)
            } else {
                Vec::new()
            }
        };

        for handle in handles {
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::sync::{RwLock, mpsc};

    use super::*;
    use crate::{EchoToolExecutor, MockLlmClient};

    /// A test persistence layer that counts GC invocations.
    #[derive(Clone, Debug, Default)]
    struct CountingPersistence {
        gc_count: Arc<AtomicU64>,
    }

    #[async_trait::async_trait]
    impl Persistence for CountingPersistence {
        async fn save_session(&self, _session_id: &str) -> Result<(), ShellError> {
            Ok(())
        }

        async fn save_plugin_blob(
            &self,
            _plugin_id: &str,
            _data: Vec<u8>,
        ) -> Result<(), ShellError> {
            Ok(())
        }

        async fn gc(&self, _session_id: &str) -> Result<u64, ShellError> {
            Ok(self.gc_count.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    #[tokio::test]
    async fn cpu_task_dispatches_registered_handler() -> Result<(), ShellError> {
        let (tx, mut rx) = mpsc::channel(4);
        let shell = BriocheShell::test_with_async_channel(tx);
        let mut registry = CpuTaskRegistry::new();
        registry.register("double", |payload: &[u8]| {
            Ok(payload.iter().flat_map(|&b| [b, b]).collect())
        });
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
                .with_cpu_task_registry(Arc::new(registry));

        executor
            .execute_cpu_task("double".into(), vec![1, 2, 3], &shell)
            .await?;

        let result = rx.recv().await.ok_or_else(|| {
            ShellError::EffectExecution("channel closed before async task result".into())
        })?;
        assert_eq!(
            result,
            AsyncTaskResult::CpuTaskDone {
                task_id: "double".into(),
                result: vec![1, 1, 2, 2, 3, 3],
            }
        );
        Ok(())
    }

    #[tokio::test]
    async fn cpu_task_falls_back_to_identity_when_unregistered() -> Result<(), ShellError> {
        let (tx, mut rx) = mpsc::channel(4);
        let shell = BriocheShell::test_with_async_channel(tx);
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);

        executor
            .execute_cpu_task("task-1".into(), vec![1, 2, 3], &shell)
            .await?;

        let result = rx.recv().await.ok_or_else(|| {
            ShellError::EffectExecution("channel closed before async task result".into())
        })?;
        assert_eq!(
            result,
            AsyncTaskResult::CpuTaskDone {
                task_id: "task-1".into(),
                result: vec![1, 2, 3],
            }
        );
        Ok(())
    }

    #[tokio::test]
    async fn summarization_emits_async_task_result() -> Result<(), ShellError> {
        let (tx, mut rx) = mpsc::channel(4);
        let shell = BriocheShell::test_with_async_channel(tx);
        let history = Arc::new(RwLock::new(vec![
            ChatMessage::User {
                content: "a".into(),
            },
            ChatMessage::User {
                content: "b".into(),
            },
            ChatMessage::User {
                content: "c".into(),
            },
            ChatMessage::User {
                content: "d".into(),
            },
            ChatMessage::User {
                content: "e".into(),
            },
        ]));
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
                .with_history(Arc::clone(&history));

        executor.trigger_summarization(&shell).await?;

        let result = rx.recv().await.ok_or_else(|| {
            ShellError::EffectExecution("channel closed before async task result".into())
        })?;
        assert!(
            matches!(
                result,
                AsyncTaskResult::SummarizationDone {
                    summary: ChatMessage::System { ref content },
                    watermark: 3,
                } if content == "Mock summary of 3 messages"
            ),
            "expected SummarizationDone for 3 summarized messages, got {:?}",
            result
        );
        Ok(())
    }

    #[tokio::test]
    async fn forward_to_ui_invokes_callback() -> Result<(), ShellError> {
        let received = Arc::new(Mutex::new(None));
        let received_clone = Arc::clone(&received);
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
                .with_ui_forwarder(move |widget: UiWidget| {
                    assert!(
                        received_clone
                            .lock()
                            .map(|mut guard| *guard = Some(widget))
                            .is_ok()
                    );
                });

        let _shell = BriocheShell::test_with_async_channel(mpsc::channel(1).0);
        let widget = UiWidget::Status("ok".into());
        executor.forward_to_ui(widget.clone()).await?;

        assert!(received.lock().is_ok_and(|guard| *guard == Some(widget)));
        Ok(())
    }

    #[tokio::test]
    async fn sub_routine_restored_invokes_callback() -> Result<(), ShellError> {
        let received = Arc::new(Mutex::new(None));
        let received_clone = Arc::clone(&received);
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
                .with_subroutine_restored_callback(move |handle: SubRoutineHandle| {
                    assert!(
                        received_clone
                            .lock()
                            .map(|mut guard| *guard = Some(handle.as_str().to_string()))
                            .is_ok()
                    );
                });

        let _shell = BriocheShell::test_with_async_channel(mpsc::channel(1).0);
        let handle = SubRoutineHandle::new("sub-42");
        executor.sub_routine_restored(handle.clone()).await?;

        assert!(
            received
                .lock()
                .is_ok_and(|guard| *guard == Some("sub-42".to_string()))
        );
        Ok(())
    }

    #[tokio::test]
    async fn trigger_gc_calls_persistence() -> Result<(), ShellError> {
        let persistence = CountingPersistence::default();
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), persistence);

        let _shell = BriocheShell::test_with_async_channel(mpsc::channel(1).0);
        executor.trigger_gc("session-a").await?;

        assert_eq!(
            executor.persistence.gc_count.load(Ordering::SeqCst),
            1,
            "gc should have been invoked once"
        );
        Ok(())
    }

    #[tokio::test]
    async fn on_system_idle_does_not_trigger_gc() -> Result<(), ShellError> {
        let persistence = CountingPersistence::default();
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), persistence);
        let _shell = BriocheShell::test_with_async_channel(mpsc::channel(1).0);

        executor.on_system_idle(&_shell, "session-b").await?;
        assert_eq!(
            executor.persistence.gc_count.load(Ordering::SeqCst),
            0,
            "on_system_idle should not trigger GC; GC is policy-driven"
        );
        Ok(())
    }

    /// A test persistence layer that records how many times `save_session` ran
    /// and can optionally sleep to simulate slow I/O.
    #[derive(Clone, Debug, Default)]
    struct RecordingPersistence {
        counter: Arc<AtomicU64>,
        delay_ms: u64,
    }

    impl RecordingPersistence {
        fn with_delay_ms(self, delay_ms: u64) -> Self {
            Self { delay_ms, ..self }
        }
    }

    #[async_trait::async_trait]
    impl Persistence for RecordingPersistence {
        async fn save_session(&self, _session_id: &str) -> Result<(), ShellError> {
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn save_plugin_blob(
            &self,
            _plugin_id: &str,
            _data: Vec<u8>,
        ) -> Result<(), ShellError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn shutdown_awaits_pending_async_saves() -> Result<(), ShellError> {
        let persistence = RecordingPersistence::default().with_delay_ms(50);
        let executor = DefaultEffectExecutor::new(
            EchoToolExecutor,
            MockLlmClient::default(),
            persistence.clone(),
        );

        executor.save_session("session-a").await?;

        // The save should be in-flight, so its handle is stored.
        assert!(
            executor
                .async_save_handles
                .lock()
                .is_ok_and(|guard| !guard.is_empty()),
            "async save handle should be stored"
        );

        // Shutdown waits for the background save to complete.
        executor.shutdown().await;

        assert_eq!(
            persistence.counter.load(Ordering::SeqCst),
            1,
            "save_session should have completed during shutdown"
        );
        assert!(
            executor
                .async_save_handles
                .lock()
                .is_ok_and(|guard| guard.is_empty()),
            "handles should be drained after shutdown"
        );
        Ok(())
    }
}
