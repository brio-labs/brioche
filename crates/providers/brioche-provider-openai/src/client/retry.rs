//! Network retry and bounded HTTP error body handling for OpenAI requests.
//!
//! This module owns transient HTTP policy and keeps provider error-body I/O
//! separate from pure request construction.
//!
//! Refs: docs/SPECS.md §Book III-B, I-Shell-Network-Signal

use std::time::Duration;

/// Retry/backoff policy for transient provider failures.
///
/// Retries are attempted for network errors, HTTP 5xx responses, and
/// HTTP 429 (rate limited) responses. The provider's `Retry-After`
/// header is honoured when present and bounded by [`RetryConfig::max_backoff_ms`].
///
/// Refs: docs/SPECS.md §Book III-B
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryConfig {
    /// Maximum number of retry attempts after the initial request.
    pub max_retries: u32,
    /// Initial backoff delay in milliseconds.
    pub base_backoff_ms: u64,
    /// Maximum backoff delay in milliseconds.
    pub max_backoff_ms: u64,
}

impl RetryConfig {
    /// Creates a retry policy with no retries.
    ///
    /// Useful in tests that need to verify first-attempt behaviour.
    ///
    /// Refs: docs/SPECS.md §Book III-B
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            base_backoff_ms: 0,
            max_backoff_ms: 0,
        }
    }

    /// Computes the backoff delay for a given retry attempt.
    ///
    /// Uses exponential growth: `base_backoff_ms * 2^(attempt - 1)`,
    /// capped at `max_backoff_ms`. Attempts are 1-indexed.
    ///
    /// # Complexity
    /// O(1).
    ///
    /// Refs: docs/SPECS.md §Book III-B
    pub fn backoff_ms(&self, attempt: u32) -> u64 {
        if self.base_backoff_ms == 0 || self.max_retries == 0 {
            return 0;
        }
        let shift = (attempt.saturating_sub(1)).min(63);
        let raw = self.base_backoff_ms.saturating_mul(1u64 << shift);
        raw.min(self.max_backoff_ms)
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 2,
            base_backoff_ms: 1_000,
            max_backoff_ms: 30_000,
        }
    }
}

/// Returns true when an HTTP status code is considered transient.
///
/// 5xx server errors and 429 rate-limit responses may resolve on retry.
/// 4xx client errors are not retried because the request itself is faulty.
///
/// Refs: docs/SPECS.md §Book III-B
pub(crate) fn is_retriable_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

/// Parses a `Retry-After` header value into a bounded millisecond delay.
///
/// Returns `None` for missing or unparseable values. The result is capped
/// at `max_delay` so a malicious provider cannot stall the shell.
///
/// Refs: docs/SPECS.md §Book III-B
pub(crate) fn parse_retry_after(
    header: Option<&reqwest::header::HeaderValue>,
    max_delay: Duration,
) -> Option<Duration> {
    let value = header?.to_str().ok()?;
    let secs: u64 = value.parse().ok()?;
    Some(Duration::from_secs(secs).min(max_delay))
}

/// Maximum number of bytes to read from an HTTP error response body.
///
/// Prevents a malicious or misbehaving provider from OOM-ing the shell by
/// returning an unbounded error payload. The limit is applied while streaming
/// chunks, so no more than this amount is buffered.
///
/// Refs: docs/SPECS.md §Book III-B
pub const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// Reads at most [`MAX_ERROR_BODY_BYTES`] from `response`, then converts the
/// buffered bytes to a string, replacing invalid UTF-8 sequences.
///
/// Streaming stops as soon as the limit is reached; remaining bytes are
/// discarded. This function consumes the response body.
///
/// Refs: docs/SPECS.md §Book III-B
pub(crate) async fn limited_error_body(mut response: reqwest::Response, limit: usize) -> String {
    let mut collected = Vec::with_capacity(limit.min(4096));
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                let remaining = limit.saturating_sub(collected.len());
                if remaining == 0 {
                    break;
                }
                let take = chunk.len().min(remaining);
                collected.extend_from_slice(&chunk[..take]);
                if collected.len() >= limit {
                    break;
                }
            }
            Ok(None) => break,
            Err(err) => {
                tracing::debug!(error = %err, "failed to read error response chunk");
                break;
            }
        }
    }
    String::from_utf8_lossy(&collected).into_owned()
}
