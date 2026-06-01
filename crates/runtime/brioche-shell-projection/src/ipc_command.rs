//! IPC command handlers for Tauri integration — Book III-C §4.
//!
//! These are plain async functions that take a [`BriocheShell`] handle
//! and perform the requested action. They contain no Tauri-specific
//! types and can be wired to `#[tauri::command]` in a downstream crate.
//!
//! ## Invariants upheld
//! - I-UI-NoUIType: No UI crate types in kernel-facing data structures.
//! - I-Shell-NoUIType: Shell projection never imports UI crates.
//! - I-UI-IPC-Rate: Rate limiting is applied at the caller boundary.
//! - I-Shell-Load-Batch: `load_subroutine` checks cache before Redb.
//!
//! Refs: SPECS.md §Book III-C Ch 4

use brioche_core::{EngineInput, SubRoutineHandle, SystemSignal};
use brioche_shell_persistence::{RedbStorage, SubRoutineCache, load_subroutine};
use brioche_shell_runtime::{BriocheShell, ShellError};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Service exposing IPC command handlers.
///
/// Holds the [`BriocheShell`] handle and optional persistence state
/// needed for sub-routine lazy loading.
///
/// # Cloning
/// `IpcCommandService` is cheaply cloneable (`Arc` internals). It can
/// be shared across Tauri command handlers and async tasks.
///
/// Refs: I-UI-NoUIType, I-Shell-Load-Batch
#[derive(Clone)]
pub struct IpcCommandService {
    shell: BriocheShell,
    storage: Option<RedbStorage>,
    cache: Option<Arc<Mutex<SubRoutineCache>>>,
}

impl IpcCommandService {
    /// Create a new IPC command service with only the shell handle.
    ///
    /// `load_subroutine` will fail if persistence is not configured
    /// via [`with_persistence`](Self::with_persistence).
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-NoUIType
    pub fn new(shell: BriocheShell) -> Self {
        Self {
            shell,
            storage: None,
            cache: None,
        }
    }

    /// Configure persistence for `load_subroutine`.
    ///
    /// The [`RedbStorage`] and [`SubRoutineCache`] are shared across
    /// clones of the service via `Arc`.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Shell-Load-Batch
    pub fn with_persistence(mut self, storage: RedbStorage, cache: SubRoutineCache) -> Self {
        self.storage = Some(storage);
        self.cache = Some(Arc::new(Mutex::new(cache)));
        self
    }

    /// Send a user message to the kernel.
    ///
    /// Injects [`EngineInput::UserMessage`] into the engine input channel.
    /// This is the primary entry point for user chat input.
    ///
    /// # Errors
    /// Returns `ShellError::RebuildInProgress` if a route recalculation
    /// barrier is active. Returns `ShellError::ChannelSend` if the
    /// engine thread has disconnected.
    ///
    /// Refs: I-UI-IPC-Rate
    pub async fn send_message(&self, text: String) -> Result<(), ShellError> {
        self.shell.send_input(EngineInput::UserMessage(text)).await
    }

    /// Cancel the current operation.
    ///
    /// Emits [`SystemSignal::OperationCancelled`] into the system signal
    /// channel, which governance plugins intercept to force an
    /// [`OverrideTransition`](brioche_core::PolicyDecision::OverrideTransition)
    /// to `Idle`.
    ///
    /// # Errors
    /// Returns `ShellError::ChannelSend` if the signal channel is closed.
    ///
    /// Refs: I-Shell-Network-Signal
    pub async fn cancel_action(&self) -> Result<(), ShellError> {
        self.shell
            .send_system_signal(SystemSignal::OperationCancelled)
            .await
    }

    /// Lazy-load a sub-routine and inject it into the kernel.
    ///
    /// 1. Checks the [`SubRoutineCache`] (L1, then L2).
    /// 2. Falls back to [`RedbStorage`] if not cached.
    /// 3. Serializes the head DTO to MessagePack.
    /// 4. Sends [`EngineInput::RestoreSubRoutine`] to the engine.
    ///
    /// Returns `Err(ShellError::EffectExecution)` if persistence is not
    /// configured or if the sub-routine is not found in cache or storage.
    ///
    /// # Panics
    /// Never panics. Returns `Err` on all failure paths.
    ///
    /// Refs: SPECS.md §Book III-C Ch 5, I-Shell-Load-Batch
    pub async fn load_subroutine(&self, handle: SubRoutineHandle) -> Result<(), ShellError> {
        let storage = self.storage.as_ref().ok_or_else(|| {
            ShellError::EffectExecution("load_subroutine requires persistence".into())
        })?;
        let cache = self
            .cache
            .as_ref()
            .ok_or_else(|| ShellError::EffectExecution("load_subroutine requires cache".into()))?;

        let mut cache_guard = cache.lock().await;
        let dto = load_subroutine(storage, &mut cache_guard, handle.as_str())
            .await
            .map_err(|e| ShellError::EffectExecution(format!("persistence error: {}", e)))?
            .ok_or_else(|| {
                ShellError::EffectExecution(format!("sub-routine {} not found", handle.as_str()))
            })?;

        // Serialize the DTO to MessagePack for the kernel.
        let head_blob = rmp_serde::to_vec(&dto)
            .map_err(|e| ShellError::EffectExecution(format!("serialize error: {}", e)))?;

        drop(cache_guard);

        self.shell
            .send_input(EngineInput::RestoreSubRoutine { handle, head_blob })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brioche_core::{BriocheEngineBuilder, Session};
    use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
    use brioche_shell_runtime::{
        DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence, ShellConfig,
    };

    /// Build a shell suitable for IPC command testing.
    ///
    /// Uses the `Permissive` governance profile and noop persistence
    /// so tests run without disk I/O.
    fn test_shell() -> BriocheShell {
        let executor =
            DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);
        BriocheShell::new(
            || {
                let engine = BriocheEngineBuilder::new()
                    .with_profile(GovernanceProfile::Permissive)
                    .build()
                    .ok()
                    .unwrap_or_else(|| unreachable!("engine build failed"));
                let session = Session::new("test-session");
                (engine, session)
            },
            ShellConfig::default(),
            executor,
            None,
        )
    }

    #[tokio::test]
    async fn send_message_injects_user_message() {
        let shell = test_shell();
        let service = IpcCommandService::new(shell);
        let result = service.send_message("hello".into()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cancel_action_emits_system_signal() {
        let shell = test_shell();
        let service = IpcCommandService::new(shell);
        let result = service.cancel_action().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn load_subroutine_without_persistence_fails() {
        let shell = test_shell();
        let service = IpcCommandService::new(shell);
        let result = service
            .load_subroutine(SubRoutineHandle::new("missing"))
            .await;
        assert!(result.is_err());
    }
}
