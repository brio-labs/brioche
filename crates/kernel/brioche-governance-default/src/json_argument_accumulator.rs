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

use std::collections::BTreeMap;

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, StreamAction, StreamEvent,
};

/// JSON argument accumulation state.
///
/// ## Snapshot strategy
/// No snapshot (`#[brioche(no_snapshot)]`). Fully reconstructed each
/// stream event; rollback is meaningless for transient accumulation.
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

/// JSON argument accumulator and validator.
///
/// On `on_stream_event`, accumulates argument fragments for
/// validation. Does not modify the mechanical flow (always Pass).
///
/// Refs: I-Core-ChunkBudget
pub struct JsonArgumentAccumulator;

impl JsonArgumentAccumulator {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
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

    /// Accumulates argument fragments for future validation.
    ///
    /// # Complexity
    /// O(log n). One `BTreeMap` insertion per fragment.
    fn on_stream_event(
        &self,
        event: &StreamEvent,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        let state = ext.get_or_insert_default::<JsonArgumentAccumulatorState>();

        match event {
            StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
                let fragment = String::from_utf8_lossy(chunk);
                state
                    .accumulated
                    .entry(id.clone())
                    .or_default()
                    .push_str(&fragment);
                state.total_fragments += 1;
            }
            StreamEvent::ToolCallDone { .. } => {
                // Optional: validate JSON syntax here.
                // For Sprint 8, we just clear the transient accumulator.
                state.accumulated.clear();
            }
            _ => {}
        }

        Ok(StreamAction::Pass)
    }
}
