//! Stream batch event channel — Book III-C §2.
//!
//! Accumulates text fragments from [`crate::ContentRenderer`] and emits them
//! as MessagePack-serialized batches at a rate-limited cadence.
//!
//! ## Invariants upheld
//! - I-UI-StreamBuffer: Batched emission avoids per-fragment VDOM churn.
//! - I-UI-IPC-Rate: Emission is gated by [`IpcRateLimiter`].
//! - I-Eco-OrderedCollections: Uses `BTreeMap` for deterministic trace ordering.
//!
//! Refs: SPECS.md §Book III-C Ch 2

use crate::IpcRateLimiter;
use serde::Serialize;
use std::collections::BTreeMap;

/// A batch of accumulated trace text, ready for MessagePack serialization.
///
/// The frontend receives this map and renders each trace in a single
/// `requestAnimationFrame` update.
///
/// # Schema
/// ```json
/// { "trace_id_1": "accumulated text", "trace_id_2": "more text" }
/// ```
///
/// Refs: I-UI-StreamBuffer
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StreamBatch {
    /// Map trace_id → accumulated text.
    pub traces: BTreeMap<String, String>,
}

impl StreamBatch {
    /// Create an empty batch.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-StreamBuffer
    pub fn new() -> Self {
        Self {
            traces: BTreeMap::new(),
        }
    }

    /// Insert or append text for a trace.
    ///
    /// Complexity: O(log n + m) where n = number of traces,
    /// m = length of the text fragment.
    ///
    /// Refs: I-UI-StreamBuffer
    pub fn append(&mut self, trace_id: impl Into<String>, text: impl AsRef<str>) {
        let id = trace_id.into();
        let txt = text.as_ref();
        match self.traces.get_mut(&id) {
            Some(existing) => existing.push_str(txt),
            None => {
                self.traces.insert(id, txt.to_owned());
            }
        }
    }

    /// Remove a trace from the batch, returning its accumulated text.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-UI-StreamBuffer
    pub fn remove(&mut self, trace_id: &str) -> Option<String> {
        self.traces.remove(trace_id)
    }

    /// Serialize the batch to MessagePack bytes.
    ///
    /// Complexity: O(serialization). Allocates one `Vec<u8>`.
    ///
    /// # Errors
    /// Returns `Err` only if the data structure contains types that
    /// `rmp_serde` cannot encode (impossible for this struct).
    ///
    /// Refs: I-UI-StreamBuffer
    pub fn to_messagepack(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec(self)
    }
}

/// Emitter that batches text fragments and emits them under rate limit.
///
/// `StreamBatchEmitter` owns an internal [`StreamBatch`] and an
/// [`IpcRateLimiter`]. Callers accumulate fragments via
/// [`accumulate`](Self::accumulate) and periodically call
/// [`try_emit`](Self::try_emit) to retrieve a MessagePack payload
/// when the frame budget allows.
///
/// Refs: I-UI-StreamBuffer, I-UI-IPC-Rate
#[derive(Clone, Debug)]
pub struct StreamBatchEmitter {
    batch: StreamBatch,
    limiter: IpcRateLimiter,
}

impl StreamBatchEmitter {
    /// Create a new emitter with the given rate limiter.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-StreamBuffer, I-UI-IPC-Rate
    pub fn new(limiter: IpcRateLimiter) -> Self {
        Self {
            batch: StreamBatch::new(),
            limiter,
        }
    }

    /// Accumulate a text fragment into the batch.
    ///
    /// Complexity: O(log n + m).
    ///
    /// Refs: I-UI-StreamBuffer
    pub fn accumulate(&mut self, trace_id: impl Into<String>, text: impl AsRef<str>) {
        self.batch.append(trace_id, text);
    }

    /// Attempt to emit the current batch if the rate limit allows.
    ///
    /// Returns `Some(Vec<u8>)` containing the MessagePack-serialized
    /// batch and clears the internal buffer. Returns `None` if the
    /// rate limiter refuses emission or the batch is empty.
    ///
    /// Complexity: O(serialization).
    ///
    /// Refs: I-UI-StreamBuffer, I-UI-IPC-Rate
    pub fn try_emit(&mut self) -> Option<Vec<u8>> {
        if self.limiter.try_emit() {
            self.force_emit()
        } else {
            None
        }
    }

    /// Force emission regardless of rate limit.
    ///
    /// This is useful for flushing the final batch before session
    /// shutdown or when the frontend explicitly requests a sync.
    ///
    /// Updates the internal [`IpcRateLimiter`] so the next
    /// [`try_emit`](Self::try_emit) is delayed by a full frame budget.
    ///
    /// Returns `Some(Vec<u8>)` if the batch is non-empty, `None`
    /// otherwise.
    ///
    /// Complexity: O(serialization).
    ///
    /// Refs: I-UI-StreamBuffer, I-UI-IPC-Rate
    pub fn force_emit(&mut self) -> Option<Vec<u8>> {
        if self.batch.traces.is_empty() {
            return None;
        }
        let bytes = self.batch.to_messagepack().ok()?;
        self.batch = StreamBatch::new();
        self.limiter.force_emit();
        Some(bytes)
    }

    /// Read-only access to the internal batch.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-StreamBuffer
    pub fn batch(&self) -> &StreamBatch {
        &self.batch
    }

    /// Returns `true` if the batch has accumulated traces.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-StreamBuffer
    pub fn has_pending(&self) -> bool {
        !self.batch.traces.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_batch_append_and_serialize() {
        let mut batch = StreamBatch::new();
        batch.append("t1", "Hello");
        batch.append("t1", " world");
        batch.append("t2", "alpha");

        let bytes = batch
            .to_messagepack()
            .unwrap_or_else(|e| unreachable!("serialize failed: {}", e));
        assert!(!bytes.is_empty());
    }

    #[test]
    fn stream_batch_remove_returns_text() {
        let mut batch = StreamBatch::new();
        batch.append("t1", "data");
        assert_eq!(batch.remove("t1"), Some("data".to_string()));
        assert_eq!(batch.remove("t1"), None);
    }

    #[test]
    fn stream_batch_deterministic_order() {
        let mut batch = StreamBatch::new();
        batch.append("z", "z");
        batch.append("a", "a");

        let keys: Vec<String> = batch.traces.keys().cloned().collect();
        assert_eq!(keys, vec!["a", "z"]);
    }

    #[test]
    fn emitter_accumulates_and_emits() {
        let limiter = IpcRateLimiter::new(0); // no budget = always allows
        let mut emitter = StreamBatchEmitter::new(limiter);

        emitter.accumulate("main", "Hello");
        emitter.accumulate("main", " world");

        let bytes = emitter
            .try_emit()
            .unwrap_or_else(|| unreachable!("should emit"));
        assert!(!bytes.is_empty());
        assert!(!emitter.has_pending());
    }

    #[test]
    fn emitter_respects_rate_limit() {
        let limiter = IpcRateLimiter::new(u64::MAX - 1); // effectively never allows
        let mut emitter = StreamBatchEmitter::new(limiter);
        emitter.accumulate("main", "text");

        // The first emission always succeeds (sentinel for "never emitted").
        assert!(emitter.force_emit().is_some());

        // Re-accumulate and verify the rate limiter now blocks.
        emitter.accumulate("main", "more");
        assert!(emitter.try_emit().is_none());
        assert!(emitter.has_pending());
    }

    #[test]
    fn emitter_force_emit_bypasses_limit() {
        let limiter = IpcRateLimiter::new(u64::MAX - 1);
        let mut emitter = StreamBatchEmitter::new(limiter);
        emitter.accumulate("main", "text");
        assert!(emitter.force_emit().is_some());
    }

    #[test]
    fn emitter_empty_returns_none() {
        let limiter = IpcRateLimiter::new(0);
        let mut emitter = StreamBatchEmitter::new(limiter);
        assert!(emitter.try_emit().is_none());
        assert!(emitter.force_emit().is_none());
    }
}
