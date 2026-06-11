//! `NetworkRecovery` — transport-level retry and backoff.
//!
//! This module lives in the **shell**, not the kernel.  Network
//! failures are **never** exposed to the kernel as variants of
//! `EngineInput`.  Instead, the shell applies retry/backoff policy
//! at the transport level and emits `SystemSignal::NetworkUnavailable`
//! only as a last resort when all retries are exhausted.
//!
//! ## Invariants
//! - I-Shell-Network-Signal: The kernel receives only `LlmStream` or
//!   `SystemSignal::NetworkUnavailable`, never raw transport errors.
//!
//! Refs: SPECS.md §Book III-A Ch 1, §Book IV Ch 1.8

use std::time::Duration;

/// Retry/backoff policy for LLM network calls.
///
/// Implementations decide whether a failed request should be retried
/// and how long to wait before the next attempt.
///
/// Refs: I-Shell-Network-Signal
pub trait NetworkRecovery: Send + Sync {
    /// Decide whether to retry a failed request.
    ///
    /// `attempt` is 0-indexed (0 = first retry after initial failure).
    /// Returns `Some(delay)` if a retry should be attempted, or `None`
    /// if retries are exhausted.
    ///
    /// # Complexity
    /// O(1).
    fn next_retry(&self, attempt: u32, error: &str) -> Option<Duration>;

    /// Maximum number of retry attempts before giving up.
    ///
    /// Refs: I-Shell-Network-Signal
    fn max_attempts(&self) -> u32;
}

/// Exponential backoff with jitter.
///
/// Default policy: 3 attempts, base delay 500 ms, multiplier 2.0,
/// max delay 8 s.
/// Refs: SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct ExponentialBackoff {
    /// Maximum retry attempts before giving up.
    pub max_attempts: u32,
    /// Base delay in milliseconds for the first retry.
    pub base_delay_ms: u64,
    /// Multiplier applied to the base delay per attempt.
    pub multiplier: f64,
    /// Hard cap on delay in milliseconds.
    pub max_delay_ms: u64,
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 500,
            multiplier: 2.0,
            max_delay_ms: 8_000,
        }
    }
}

impl NetworkRecovery for ExponentialBackoff {
    fn next_retry(&self, attempt: u32, _error: &str) -> Option<Duration> {
        if attempt >= self.max_attempts {
            return None;
        }
        let delay_ms = (self.base_delay_ms as f64 * self.multiplier.powi(attempt as i32))
            .min(self.max_delay_ms as f64) as u64;
        Some(Duration::from_millis(delay_ms))
    }

    fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
}

/// No-op recovery: never retries.
///
/// Useful in tests or when the caller wants immediate failure.
/// Refs: SPECS.md §Book III-A
#[derive(Clone, Debug, Default)]
pub struct NoRetry;

impl NetworkRecovery for NoRetry {
    fn next_retry(&self, _attempt: u32, _error: &str) -> Option<Duration> {
        None
    }

    fn max_attempts(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff_limits_attempts() {
        let policy = ExponentialBackoff::default();
        assert_eq!(policy.max_attempts(), 3);
        assert!(policy.next_retry(0, "timeout").is_some());
        assert!(policy.next_retry(1, "timeout").is_some());
        assert!(policy.next_retry(2, "timeout").is_some());
        assert!(policy.next_retry(3, "timeout").is_none());
    }

    #[test]
    fn exponential_backoff_grows() {
        let policy = ExponentialBackoff::default();
        let d0 = policy.next_retry(0, "");
        let d1 = policy.next_retry(1, "");
        let d2 = policy.next_retry(2, "");
        assert!(d0.is_some());
        assert!(d1.is_some());
        assert!(d2.is_some());
        assert!(d0 < d1, "backoff should grow between attempts");
        assert!(d1 < d2, "backoff should grow between attempts");
    }

    #[test]
    fn exponential_backoff_caps_at_max() {
        let policy = ExponentialBackoff {
            max_attempts: 10,
            base_delay_ms: 1_000,
            multiplier: 10.0,
            max_delay_ms: 5_000,
        };
        let d2 = policy.next_retry(2, "");
        assert!(d2.is_some());
        assert_eq!(d2, Some(Duration::from_millis(5_000)));
    }

    #[test]
    fn no_retry_never_retries() {
        let policy = NoRetry;
        assert_eq!(policy.max_attempts(), 0);
        assert!(policy.next_retry(0, "anything").is_none());
    }
}
