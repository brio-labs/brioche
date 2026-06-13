//! Effect execution dispatch.
//!
//! The [`EffectExecutor`] trait is the boundary between the kernel's
//! declarative `Effect` enum and the shell's async I/O subsystems.
//!
//! A default implementation [`DefaultEffectExecutor`] is provided; it
//! delegates to pluggable traits (`ToolExecutor`, `LlmClient`, `Persistence`).
//!
//! Refs: I-Shell-Runtime-OnlyIO, I-Shell-ToolResult-PassThrough

use std::sync::Arc;

use brioche_core::{
    ActiveToolCall, ChatMessage, ErrorCode, ErrorDetail, SubRoutineHandle, SystemSignal,
    ToolResultDTO, UiWidget,
};

use crate::{BriocheShell, NetworkRecovery, PersistenceMode, ShellError};

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
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), ShellError>;

    /// Execute tools in parallel and send `EngineInput::ToolCallsResult`
    /// back to the kernel.
    ///
    /// `generation_id` is captured from the engine state snapshot so the
    /// shell never reads `Session` directly.
    async fn execute_tools(
        &self,
        calls: Vec<ActiveToolCall>,
        generation_id: u64,
        shell: &BriocheShell,
    ) -> Result<(), ShellError>;

    /// Forward a structured UI widget to the presentation layer.
    async fn forward_to_ui(&self, widget: UiWidget) -> Result<(), ShellError>;

    /// Log an error effect (telemetry / tracing).
    async fn log_error(&self, code: ErrorCode, detail: ErrorDetail) -> Result<(), ShellError>;

    /// Persist session state.
    async fn save_session(&self, session_id: &str) -> Result<(), ShellError>;

    /// Persist a plugin blob.
    async fn save_plugin_blob(&self, plugin_id: &str, data: Vec<u8>) -> Result<(), ShellError>;

    /// Trigger a background summarization task.
    async fn trigger_summarization(&self, shell: &BriocheShell) -> Result<(), ShellError>;

    /// Execute a CPU-intensive task on the blocking thread pool.
    async fn execute_cpu_task(
        &self,
        task_id: String,
        payload: Vec<u8>,
        shell: &BriocheShell,
    ) -> Result<(), ShellError>;

    /// Trigger opportunistic garbage collection.
    async fn trigger_gc(&self) -> Result<(), ShellError>;

    /// Handle `SystemIdle` — may decide to trigger GC.
    async fn on_system_idle(&self, shell: &BriocheShell) -> Result<(), ShellError>;

    /// Rebuild routing tables (transactional barrier).
    async fn rebuild_routes(&self) -> Result<(), ShellError>;

    /// Notify that a sub-routine has been restored.
    async fn sub_routine_restored(&self, handle: SubRoutineHandle) -> Result<(), ShellError>;
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
}

impl<T, L, P> Clone for DefaultEffectExecutor<T, L, P> {
    fn clone(&self) -> Self {
        Self {
            tool_executor: Arc::clone(&self.tool_executor),
            llm_client: Arc::clone(&self.llm_client),
            persistence: Arc::clone(&self.persistence),
            persistence_mode: self.persistence_mode,
            network_recovery: self.network_recovery.as_ref().map(Arc::clone),
        }
    }
}

impl<T, L, P> DefaultEffectExecutor<T, L, P> {
    /// Create a new executor with the given subsystems.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(tool_executor: T, llm_client: L, persistence: P) -> Self {
        Self {
            tool_executor: Arc::new(tool_executor),
            llm_client: Arc::new(llm_client),
            persistence: Arc::new(persistence),
            persistence_mode: PersistenceMode::Async,
            network_recovery: None,
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

    async fn forward_to_ui(&self, _widget: UiWidget) -> Result<(), ShellError> {
        // In Sprint 9 this is a no-op; Shell Projection (Sprint 14)
        // will wire this to the Tauri IPC layer.
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
                // Non-blocking: spawn the save on a background task.
                tokio::spawn(async move {
                    if let Err(err) = persistence.save_session(&id).await {
                        tracing::error!(error = %err, "async save_session failed");
                    }
                });
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
        // Background summarization: invoke a lightweight LLM call and
        // emit AsyncTaskResult::SummarizationDone when complete.
        // Sprint 9 placeholder: immediately emit a dummy result.
        let summary = ChatMessage::System {
            content: "[summarization placeholder]".into(),
        };
        let result = brioche_core::AsyncTaskResult::SummarizationDone {
            summary,
            watermark: 0,
        };
        // In a full implementation the LLM client would perform the
        // summarization and then send the result via the async task
        // result channel. For Sprint 9 we inject directly.
        shell
            .send_input(brioche_core::EngineInput::UserMessage(
                "[summarization complete]".into(),
            ))
            .await?;
        let _ = result;
        Ok(())
    }

    async fn execute_cpu_task(
        &self,
        task_id: String,
        payload: Vec<u8>,
        shell: &BriocheShell,
    ) -> Result<(), ShellError> {
        let result = tokio::task::spawn_blocking(move || {
            // Placeholder: identity transformation.
            payload
        })
        .await
        .map_err(|e| ShellError::EffectExecution(format!("cpu task panicked: {}", e)))?;

        shell
            .send_input(brioche_core::EngineInput::UserMessage(format!(
                "[cpu task {} done]",
                task_id
            )))
            .await?;
        let _ = result;
        Ok(())
    }

    async fn trigger_gc(&self) -> Result<(), ShellError> {
        // Sprint 12 will implement opportunistic GC via the persistence layer.
        // Sprint 9 placeholder: log and return.
        tracing::info!("GC triggered (placeholder)");
        Ok(())
    }

    async fn on_system_idle(&self, _shell: &BriocheShell) -> Result<(), ShellError> {
        // GcPolicy (Sprint 16) will decide on TriggerGc after SystemIdle.
        // Sprint 9: no automatic GC decision.
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

    async fn sub_routine_restored(&self, _handle: SubRoutineHandle) -> Result<(), ShellError> {
        // Sprint 13 will update SubRoutineCache (L1/L2).
        // Sprint 9 placeholder.
        Ok(())
    }
}
