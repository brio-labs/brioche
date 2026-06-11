//! Input interception & validation plugins.
//!
//! This module groups plugins that inspect or validate incoming input:
//! - `DepthGuard`: limits sub-routine nesting depth.
//! - `JsonArgumentAccumulator`: validates streamed tool argument fragments.
//!
//! Refs: I-Gov-Depth-Limit, I-Core-ChunkBudget

use std::collections::BTreeMap;

use brioche_core::{
    AgentStateTag, BriochePlugin, Effect, EngineInput, ErrorCode, ErrorDetail, ExtensionStorage,
    PluginCapabilities, PluginResult, PolicyDecision, SessionSnapshot, StreamAction, StreamEvent,
};

// ---------------------------------------------------------------------------
// DepthGuard
// ---------------------------------------------------------------------------

/// Current nesting depth, tracked in `ExtensionStorage`.
///
/// ## Snapshot strategy
/// COW: full clone (~8 bytes). One scalar field.
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
#[brioche(critical_state)]
pub struct DepthState {
    /// Current depth (derived from `state_stack_depth` on the fly).
    pub current_depth: u64,
}

/// Sub-routine depth guard.
///
/// On `on_input`, verifies that the stack depth does not exceed
/// `max_depth`. If so, emits an `OverrideTransition` to
/// `Idle` with a UI notification.
///
/// Refs: I-Gov-Depth-Limit
pub struct DepthGuard {
    max_depth: u64,
}

impl DepthGuard {
    /// Creates a guard with a maximum depth.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_max_depth(max_depth: u64) -> Self {
        Self { max_depth }
    }
}

impl Default for DepthGuard {
    fn default() -> Self {
        Self::with_max_depth(10)
    }
}

/// Pure function: calculates the effective sub-routine nesting depth.
///
/// Hooks orchestrate; functions compute. This is unit-testable without
/// `ExtensionStorage` mocks.
///
/// Refs: I-Gov-TraitAtomic
/// Refs: I-Comp-Pure-Logic
pub fn calculate_depth(stack_depth: usize, current_state: AgentStateTag) -> u64 {
    if current_state == AgentStateTag::SubRoutine {
        stack_depth as u64 + 1
    } else {
        stack_depth as u64
    }
}

impl BriochePlugin for DepthGuard {
    fn name(&self) -> &'static str {
        "depth_guard"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        -40
    }

    /// Verifies depth before each `UserMessage`.
    ///
    /// # Complexity
    /// O(log n). Two `ExtensionStorage` reads.
    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        if !matches!(input, EngineInput::UserMessage(_)) {
            return Ok(PolicyDecision::Allow);
        }

        let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
        let current_depth = calculate_depth(snapshot.state_stack_depth, snapshot.current_state);

        let state = ext.get_or_insert_default::<DepthState>();
        state.current_depth = current_depth;

        if current_depth >= self.max_depth {
            return Ok(PolicyDecision::OverrideTransition(vec![
                Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    detail: ErrorDetail::Generic(format!(
                        "sub-routine depth limit exceeded: {} >= {}",
                        current_depth, self.max_depth
                    )),
                },
                Effect::ForwardToUi(brioche_core::UiWidget::Error {
                    code: "DEPTH_LIMIT_EXCEEDED".into(),
                    message: format!(
                        "sub-routine depth limit exceeded: {} >= {}",
                        current_depth, self.max_depth
                    ),
                }),
                Effect::SystemIdle,
            ]));
        }

        Ok(PolicyDecision::Allow)
    }
}

// ---------------------------------------------------------------------------
// JsonArgumentAccumulator
// ---------------------------------------------------------------------------

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
/// Refs: I-Gov-TraitAtomic
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
