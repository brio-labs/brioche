//! StreamBuffer — Book III-C §2
//!
//! Accumulates streaming text fragments by trace identifier.
//! The frontend consumes the buffer on `requestAnimationFrame` ticks,
//! rendering the accumulated text in a single update per frame.
//!
//! ## Invariants upheld
//! - I-UI-StreamBuffer: Fragments accumulate outside granular reactivity.
//! - I-Eco-OrderedCollections: Uses `BTreeMap` for deterministic trace ordering.
//!
//! Refs: SPECS.md §Book III-C Ch 2

use std::collections::BTreeMap;

/// Single source of truth for partial streaming text.
///
/// `StreamBuffer` maps `trace_id` → accumulated text. The frontend
/// polls this structure (or its serialized representation) once per
/// frame and renders the full accumulated string, avoiding per-fragment
/// VDOM mutations.
///
/// # Determinism
/// Uses `BTreeMap` so iteration order over traces is deterministic.
///
/// Refs: I-UI-StreamBuffer, I-Eco-OrderedCollections
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamBuffer {
    buffers: BTreeMap<String, String>,
}

impl StreamBuffer {
    /// Create an empty buffer.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn new() -> Self {
        Self {
            buffers: BTreeMap::new(),
        }
    }

    /// Append a text fragment to the buffer for the given trace.
    ///
    /// If `trace_id` does not yet exist, a new entry is created.
    ///
    /// Complexity: O(log n + m) where n = number of traces,
    /// m = length of fragment (amortized `String` append).
    /// Refs: SPECS.md §Book III-A
    pub fn append(&mut self, trace_id: impl Into<String>, fragment: impl AsRef<str>) {
        let id = trace_id.into();
        let frag = fragment.as_ref();
        match self.buffers.get_mut(&id) {
            Some(existing) => existing.push_str(frag),
            None => {
                self.buffers.insert(id, frag.to_owned());
            }
        }
    }

    /// Read the accumulated text for a trace without consuming it.
    ///
    /// Complexity: O(log n).
    /// Refs: SPECS.md §Book III-A
    pub fn get(&self, trace_id: &str) -> Option<&str> {
        self.buffers.get(trace_id).map(|s| s.as_str())
    }

    /// Consume and return the accumulated text for a trace, removing it.
    ///
    /// Returns `None` if the trace is not present.
    ///
    /// Complexity: O(log n).
    /// Refs: SPECS.md §Book III-A
    pub fn flush(&mut self, trace_id: &str) -> Option<String> {
        self.buffers.remove(trace_id)
    }

    /// Remove all traces from the buffer.
    ///
    /// Complexity: O(n) where n = number of traces.
    /// Refs: SPECS.md §Book III-A
    pub fn clear(&mut self) {
        self.buffers.clear();
    }

    /// Iterate over all traces in deterministic order.
    ///
    /// Complexity: O(1) for iterator creation.
    /// Refs: SPECS.md §Book III-A
    pub fn traces(&self) -> impl Iterator<Item = (&String, &String)> {
        self.buffers.iter()
    }

    /// Total number of active traces.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Returns `true` if no traces are buffered.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}
