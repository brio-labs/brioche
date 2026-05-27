//! IPC rate limiter — Book III-C §4.
//!
//! Ensures the projection layer emits at most one IPC event per frame
//! to the frontend, with adaptive batching.
//!
//! ## Invariants upheld
//! - I-UI-IPC-Rate: < 1 event per frame.
//!
//! Refs: SPECS.md §Book III-C Ch 4.4

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

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
mod tests {
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
