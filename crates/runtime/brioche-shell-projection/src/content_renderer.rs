//! ContentRenderer — Book III-C §2
//!
//! Streaming rendering engine that processes `Effect::ForwardToUi`
//! instructions and accumulates text fragments into a `StreamBuffer`.
//! The frontend reads the buffer on `requestAnimationFrame` ticks.
//!
//! ## Invariants upheld
//! - I-UI-StreamBuffer: Partial text is accumulated, not rendered per fragment.
//! - I-UI-NoDirectDOM: Rust side never touches DOM; only produces data.
//!
//! Refs: SPECS.md §Book III-C Ch 2

use brioche_core::Effect;

use crate::StreamBuffer;

/// Streaming engine that accumulates text fragments by trace ID.
///
/// `ContentRenderer` owns a [`StreamBuffer`] and exposes a method to
/// ingest `ForwardToUi` effects. Only effects whose `widget_type` is
/// [`WIDGET_TEXT_CHUNK`](crate::widget::WIDGET_TEXT_CHUNK) are consumed;
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
    pub fn buffer(&self) -> &StreamBuffer {
        &self.buffer
    }

    /// Mutable access to the internal stream buffer.
    ///
    /// Primarily used by tests and by the shell when manually injecting
    /// text (e.g. from sub-routine hydration).
    ///
    /// Complexity: O(1).
    pub fn buffer_mut(&mut self) -> &mut StreamBuffer {
        &mut self.buffer
    }

    /// Consume and return the accumulated text for a trace, if any.
    ///
    /// This is the primary mechanism for the frontend to drain a trace
    /// after it has been rendered.
    ///
    /// Complexity: O(log n).
    pub fn drain_trace(&mut self, trace_id: &str) -> Option<String> {
        self.buffer.flush(trace_id)
    }

    /// Remove all traces from the internal buffer.
    ///
    /// Called by the shell on session reset or sub-routine cleanup.
    ///
    /// Complexity: O(n) where n = number of traces.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
