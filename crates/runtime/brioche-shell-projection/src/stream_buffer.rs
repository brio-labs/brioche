//! StreamBuffer and ContentRenderer — Book III-C §2
//!
//! `StreamBuffer` accumulates streaming text fragments by trace identifier.
//! `ContentRenderer` wraps the buffer and processes `Effect::ForwardToUi`
//! instructions, filtering only `UiWidget::TextChunk` variants.
//!
//! The frontend consumes the buffer on `requestAnimationFrame` ticks,
//! rendering the accumulated text in a single update per frame.
//!
//! ## Invariants upheld
//! - I-UI-StreamBuffer: Fragments accumulate outside granular reactivity.
//! - I-Eco-OrderedCollections: Uses `BTreeMap` for deterministic trace ordering.
//! - I-UI-NoDirectDOM: Rust side never touches DOM; only produces data.
//!
//! Refs: docs/SPECS.md §Book III-C Ch 2

use std::collections::BTreeMap;

use brioche_core::Effect;

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
    /// Refs: docs/SPECS.md §Book III-A
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
    /// Refs: docs/SPECS.md §Book III-A
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
    /// Refs: docs/SPECS.md §Book III-A
    pub fn get(&self, trace_id: &str) -> Option<&str> {
        self.buffers.get(trace_id).map(|s| s.as_str())
    }

    /// Consume and return the accumulated text for a trace, removing it.
    ///
    /// Returns `None` if the trace is not present.
    ///
    /// Complexity: O(log n).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn flush(&mut self, trace_id: &str) -> Option<String> {
        self.buffers.remove(trace_id)
    }

    /// Remove all traces from the buffer.
    ///
    /// Complexity: O(n) where n = number of traces.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn clear(&mut self) {
        self.buffers.clear();
    }

    /// Iterate over all traces in deterministic order.
    ///
    /// Complexity: O(1) for iterator creation.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn traces(&self) -> impl Iterator<Item = (&String, &String)> {
        self.buffers.iter()
    }

    /// Total number of active traces.
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Returns `true` if no traces are buffered.
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ContentRenderer (merged from content_renderer.rs)
// ---------------------------------------------------------------------------

/// Streaming engine that accumulates text fragments by trace ID.
///
/// `ContentRenderer` owns a [`StreamBuffer`] and exposes a method to
/// ingest `ForwardToUi` effects. Only effects whose `widget_type` is
/// [`WIDGET_TEXT_CHUNK`](crate::ui_registry::WIDGET_TEXT_CHUNK) are consumed;
/// all other effects are returned unchanged.
///
/// # Usage
/// The shell's effect executor passes `ForwardToUi` effects through
/// the renderer before handing them to the `UiComposer`. Text chunks
/// are absorbed into the buffer; non-text effects pass through.
///
/// Refs: I-UI-StreamBuffer
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentRenderer {
    buffer: StreamBuffer,
}

impl ContentRenderer {
    /// Create a new renderer with an empty buffer.
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new() -> Self {
        Self {
            buffer: StreamBuffer::new(),
        }
    }

    /// Process a single `Effect`.
    ///
    /// If the effect is `ForwardToUi(UiWidget::TextChunk { .. })`,
    /// the `trace_id` and `text` fields are extracted and the fragment is
    /// appended to the internal buffer. The method returns
    /// `true` to indicate consumption.
    ///
    /// For all other effects, the method returns `false` and the caller
    /// must forward the effect onward.
    ///
    /// # Payload
    /// `UiWidget::TextChunk { trace_id, text }`
    ///
    /// Complexity: O(log n + m) where n = traces in buffer,
    /// m = text fragment length.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn process_effect(&mut self, effect: &Effect) -> bool {
        let Effect::ForwardToUi(widget) = effect else {
            return false;
        };

        let (trace_id, text) = match widget {
            brioche_core::UiWidget::TextChunk { trace_id, text } => {
                (trace_id.as_str(), text.as_str())
            }
            _ => return false,
        };

        self.buffer.append(trace_id, text);
        true
    }

    /// Read-only access to the internal stream buffer.
    ///
    /// The frontend (or IPC serializer) calls this to extract accumulated
    /// text for rendering.
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn buffer(&self) -> &StreamBuffer {
        &self.buffer
    }

    /// Mutable access to the internal stream buffer.
    ///
    /// Primarily used by tests and by the shell when manually injecting
    /// text (e.g. from sub-routine hydration).
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn buffer_mut(&mut self) -> &mut StreamBuffer {
        &mut self.buffer
    }

    /// Consume and return the accumulated text for a trace, if any.
    ///
    /// This is the primary mechanism for the frontend to drain a trace
    /// after it has been rendered.
    ///
    /// Complexity: O(log n).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn drain_trace(&mut self, trace_id: &str) -> Option<String> {
        self.buffer.flush(trace_id)
    }

    /// Remove all traces from the internal buffer.
    ///
    /// Called by the shell on session reset or sub-routine cleanup.
    ///
    /// Complexity: O(n) where n = number of traces.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
