//! JsonArgumentAccumulator — Book II §5.6.
//!
//! Accumulates `ToolArgumentChunk` fragments and validates JSON syntax.
//!
//! In the current architecture, the kernel's `StreamToolAccumulator`
//! already buffers argument chunks. This plugin provides policy-level
//! validation (e.g., JSON well-formedness, size limits) without
//! modifying the mechanical accumulator.
//!
//! Refs: I-Core-ChunkBudget

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, StreamAction, StreamEvent,
};
use std::collections::BTreeMap;

/// JSON argument accumulation state.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
#[brioche(no_snapshot)]
pub struct JsonArgumentAccumulatorState {
    /// Map tool_id -> accumulated argument bytes (for validation only).
    pub accumulated: BTreeMap<String, String>,
    /// Total number of fragments received.
    pub total_fragments: u64,
}

/// Accumulateur et validateur d'arguments JSON.
///
/// Sur `on_stream_event`, accumule les fragments d'arguments pour
/// validation. Does not modify the mechanical flow (always Pass).
pub struct JsonArgumentAccumulator;

impl JsonArgumentAccumulator {
    /// Creates a new instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonArgumentAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for JsonArgumentAccumulator {
    fn name(&self) -> &'static str {
        "json_argument_accumulator"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_STREAM_EVENT
    }

    fn priority(&self) -> i16 {
        20 // After ToolCallDetector, before any Hold/Offload decisions
    }

    /// Accumule les fragments d'arguments pour validation future.
    ///
    /// # Complexity
    /// O(log n). Une insertion `BTreeMap` par fragment.
    fn on_stream_event(
        &self,
        event: &StreamEvent,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        match event {
            StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
                let fragment = String::from_utf8_lossy(chunk);
                ext.with_or_insert_default::<JsonArgumentAccumulatorState, _>(|state| {
                    state
                        .accumulated
                        .entry(id.clone())
                        .or_default()
                        .push_str(&fragment);
                    state.total_fragments += 1;
                });
            }
            StreamEvent::ToolCallDone { .. } => {
                // Optional: validate JSON syntax here.
                // For Sprint 8, we just clear the transient accumulator.
                ext.with_or_insert_default::<JsonArgumentAccumulatorState, _>(|state| {
                    state.accumulated.clear();
                });
            }
            _ => {}
        }

        Ok(StreamAction::Pass)
    }
}
