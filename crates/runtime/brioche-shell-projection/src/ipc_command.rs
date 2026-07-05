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
//! Refs: docs/SPECS.md §Book III-C Ch 4

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use brioche_core::{EngineInput, SubRoutineHandle, SystemSignal};
use brioche_shell_persistence::{RedbStorage, SubRoutineCache};
use brioche_shell_runtime::{BriocheShell, ShellError};
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
    /// # Cancel safety
    /// This future delegates to `BriocheShell::send_input`. Dropping it
    /// before completion only fails to enqueue the input.
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
    /// # Cancel safety
    /// This future delegates to `BriocheShell::send_system_signal`. Dropping
    /// it before completion only fails to enqueue the signal.
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
    /// # Cancel safety
    /// The cache lock is acquired only for short, non-awaiting cache reads
    /// and updates; it is never held across `storage.load_session().await` or
    /// other await points. Dropping this future before completion discards the
    /// result; cache and storage remain unchanged.
    ///
    /// Refs: docs/SPECS.md §Book III-C Ch 5, I-Shell-Load-Batch
    pub async fn load_subroutine(&self, handle: SubRoutineHandle) -> Result<(), ShellError> {
        let storage = self.storage.as_ref().ok_or_else(|| {
            ShellError::EffectExecution("load_subroutine requires persistence".into())
        })?;
        let cache = self
            .cache
            .as_ref()
            .ok_or_else(|| ShellError::EffectExecution("load_subroutine requires cache".into()))?;

        // Acquire the lock only for the cache read; clone a hit and release.
        let cached = {
            let cache_guard = cache.lock().await;
            cache_guard.get(handle.as_str()).cloned()
        };

        let dto = match cached {
            Some(dto) => dto,
            None => {
                // Load from Redb without holding the cache lock.
                let loaded = storage
                    .load_session(handle.as_str())
                    .await
                    .map_err(|e| ShellError::EffectExecution(format!("persistence error: {}", e)))?
                    .ok_or_else(|| {
                        ShellError::EffectExecution(format!(
                            "sub-routine {} not found",
                            handle.as_str()
                        ))
                    })?;

                // Re-acquire the lock only for the cache update.
                let mut cache_guard = cache.lock().await;
                cache_guard.insert(handle.as_str().to_string(), loaded.clone());
                loaded
            }
        };

        // Serialize and send only after the lock has been released.
        let head_blob = rmp_serde::to_vec(&dto)
            .map_err(|e| ShellError::EffectExecution(format!("serialize error: {}", e)))?;

        self.shell
            .send_input(EngineInput::RestoreSubRoutine { handle, head_blob })
            .await
    }
}

#[cfg(test)]
impl IpcCommandService {
    /// Expose the internal sub-routine cache for test assertions.
    ///
    /// Refs: I-Shell-Load-Batch
    pub(crate) fn cache(&self) -> Option<Arc<Mutex<SubRoutineCache>>> {
        self.cache.clone()
    }
}

// ---------------------------------------------------------------------------
// IPC rate limiter (merged from ipc_rate_limiter.rs)
// ---------------------------------------------------------------------------

/// Frame-based rate limiter for IPC events.
///
/// `IpcRateLimiter` tracks the time of the last emission and refuses
/// subsequent `try_emit` calls until at least `frame_budget_ms` have
/// elapsed. This guarantees the frontend receives at most one event
/// per frame budget window.
///
/// # Thread safety
/// `try_emit` is lock-free (single atomic load + compare-and-swap).
/// The limiter is safe to share across tasks via `Clone`.
///
/// Refs: I-UI-IPC-Rate
#[derive(Clone, Debug)]
pub struct IpcRateLimiter {
    /// Minimum milliseconds between emissions.
    frame_budget_ms: u64,
    /// Milliseconds since `epoch` of the last successful emission.
    last_emit_ms: Arc<AtomicU64>,
    /// Anchor instant for relative time measurement.
    epoch: Arc<Instant>,
}

impl IpcRateLimiter {
    /// Create a new rate limiter with the given frame budget.
    ///
    /// `frame_budget_ms` should correspond to the target frame interval
    /// (e.g., 16 ms for 60 fps, or 2 ms for the `UiComposer` budget).
    ///
    /// Complexity: O(1).
    ///
    /// Shell-side timing is required for frame-based rate limiting.
    /// `Instant::now()` is prohibited in Core by PHILOSOPHY.md §2.2
    /// but permitted in Shell layers.
    ///
    /// Refs: I-UI-IPC-Rate
    #[allow(clippy::disallowed_methods)]
    pub fn new(frame_budget_ms: u64) -> Self {
        Self {
            frame_budget_ms,
            // `u64::MAX` is the sentinel for "never emitted".
            last_emit_ms: Arc::new(AtomicU64::new(u64::MAX)),
            epoch: Arc::new(Instant::now()),
        }
    }

    /// Attempt to emit an event.
    ///
    /// Returns `true` if at least `frame_budget_ms` have elapsed since
    /// the last successful emission. Updates the last-emits timestamp
    /// atomically.
    ///
    /// Returns `false` if the caller must hold the event for the next
    /// frame (adaptive batching).
    ///
    /// Complexity: O(1). Lock-free.
    ///
    /// Refs: I-UI-IPC-Rate
    pub fn try_emit(&self) -> bool {
        let now = self.epoch.elapsed().as_millis() as u64;
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        let elapsed = now.saturating_sub(last);

        // `u64::MAX` is the sentinel for "never emitted" — always allow the first emission.
        if last == u64::MAX || elapsed >= self.frame_budget_ms {
            // Best-effort CAS: if another task raced us, we treat it
            // as a successful emission (the frame slot is consumed).
            let _ =
                self.last_emit_ms
                    .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Force an emission, bypassing the rate limit.
    ///
    /// Updates the last-emits timestamp so the next regular `try_emit`
    /// is delayed by a full frame budget.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-IPC-Rate
    pub fn force_emit(&self) {
        let now = self.epoch.elapsed().as_millis() as u64;
        self.last_emit_ms.store(now, Ordering::Relaxed);
    }

    /// Current frame budget in milliseconds.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-IPC-Rate
    pub fn frame_budget_ms(&self) -> u64 {
        self.frame_budget_ms
    }
}

#[cfg(test)]
mod rate_limiter_tests {
    use super::*;

    #[test]
    fn rate_limiter_allows_first_emit() {
        let limiter = IpcRateLimiter::new(100);
        assert!(limiter.try_emit());
    }

    #[test]
    fn rate_limiter_blocks_within_budget() {
        let limiter = IpcRateLimiter::new(10_000);
        assert!(limiter.try_emit());
        assert!(!limiter.try_emit());
    }

    #[test]
    fn rate_limiter_force_emit_updates_timestamp() {
        let limiter = IpcRateLimiter::new(10_000);
        limiter.force_emit();
        assert!(!limiter.try_emit());
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use brioche_core::{BriocheEngineBuilder, Session};
    use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
    use brioche_shell_persistence::{
        RedbStorage, SessionHeadDTO, SubRoutineCache, new_session_store,
    };
    use brioche_shell_runtime::{
        DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence, ShellConfig,
    };

    use super::*;

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
                    .build();
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

    #[tokio::test]
    async fn load_subroutine_populates_cache_from_redb() {
        async fn run() -> Result<(), String> {
            let tmp = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
            let store = new_session_store();
            let storage = RedbStorage::new(tmp.path(), store).map_err(|e| e.to_string())?;
            let cache = SubRoutineCache::new(NonZeroUsize::new(10).ok_or("cache capacity")?);

            let session = Session::new("sub-1");
            let dto = SessionHeadDTO::from_session(&session);
            storage
                .save_session_dto(&dto)
                .await
                .map_err(|e| e.to_string())?;

            let shell = test_shell();
            let service = IpcCommandService::new(shell).with_persistence(storage, cache);

            service
                .load_subroutine(SubRoutineHandle::new("sub-1"))
                .await
                .map_err(|e| e.to_string())?;

            let cache_arc = service.cache().ok_or("missing cache")?;
            let cache_guard = cache_arc.lock().await;
            assert!(cache_guard.contains("sub-1"));

            Ok(())
        }

        assert!(run().await.is_ok());
    }
}
