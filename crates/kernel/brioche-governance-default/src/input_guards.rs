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

use crate::Priority;

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
pub fn calculate_depth(stack_depth: u64, current_state: AgentStateTag) -> u64 {
    if current_state == AgentStateTag::SubRoutine {
        stack_depth + 1
    } else {
        stack_depth
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
        Priority::DEPTH_GUARD
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
                    detail: ErrorDetail::EpochGuardRejected {
                        reason: format!(
                            "sub-routine depth limit exceeded: {} >= {}",
                            current_depth, self.max_depth
                        ),
                    },
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
        Priority::ARGUMENT_ACCUMULATOR // After ToolCallDetector, before any Hold/Offload decisions
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

#[cfg(test)]
mod tests {
    use brioche_core::{
        AgentStateTag, EngineInput, ExtensionStorage, PluginError, SessionSnapshot,
    };

    use super::*;

    #[test]
    fn calculate_depth_adds_one_in_subroutine() {
        assert_eq!(calculate_depth(5, AgentStateTag::SubRoutine), 6);
    }

    #[test]
    fn calculate_depth_matches_stack_for_other_states() {
        assert_eq!(calculate_depth(5, AgentStateTag::Idle), 5);
        assert_eq!(calculate_depth(5, AgentStateTag::Predicting), 5);
        assert_eq!(calculate_depth(5, AgentStateTag::ExecutingTools), 5);
        assert_eq!(calculate_depth(5, AgentStateTag::Failure), 5);
    }

    #[test]
    fn depth_guard_blocks_at_limit() -> Result<(), PluginError> {
        let guard = DepthGuard::with_max_depth(5);
        let mut ext = ExtensionStorage::new();
        {
            let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
            snapshot.state_stack_depth = 4;
            snapshot.current_state = AgentStateTag::SubRoutine;
        }

        let decision = guard.on_input(&EngineInput::UserMessage("deep".into()), &mut ext)?;

        assert!(
            matches!(decision, PolicyDecision::OverrideTransition(_)),
            "depth guard should override transition when limit reached"
        );

        let state = ext.get_or_insert_default::<DepthState>();
        assert_eq!(state.current_depth, 5);
        Ok(())
    }

    #[test]
    fn depth_guard_allows_below_limit() -> Result<(), PluginError> {
        let guard = DepthGuard::with_max_depth(5);
        let mut ext = ExtensionStorage::new();
        {
            let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
            snapshot.state_stack_depth = 3;
            snapshot.current_state = AgentStateTag::SubRoutine;
        }

        let decision = guard.on_input(&EngineInput::UserMessage("ok".into()), &mut ext)?;

        assert!(matches!(decision, PolicyDecision::Allow));

        let state = ext.get_or_insert_default::<DepthState>();
        assert_eq!(state.current_depth, 4);
        Ok(())
    }

    #[test]
    fn json_argument_accumulator_collects_fragments() -> Result<(), PluginError> {
        let accumulator = JsonArgumentAccumulator::new();
        let mut ext = ExtensionStorage::new();
        let path = brioche_core::ExecutionPath::default();

        let events = vec![
            StreamEvent::ToolArgumentChunk {
                path: path.clone(),
                id: "tc1".into(),
                chunk: From::from(&b"{"[..]),
            },
            StreamEvent::ToolArgumentChunk {
                path,
                id: "tc1".into(),
                chunk: From::from(&b"\"x\":1}"[..]),
            },
        ];

        for event in &events {
            let action = accumulator.on_stream_event(event, &mut ext)?;
            assert!(matches!(action, StreamAction::Pass));
        }

        let state = ext.get_or_insert_default::<JsonArgumentAccumulatorState>();
        assert_eq!(state.accumulated.get("tc1"), Some(&"{\"x\":1}".to_string()));
        assert_eq!(state.total_fragments, 2);
        Ok(())
    }

    #[test]
    fn json_argument_accumulator_clears_on_done() -> Result<(), PluginError> {
        let accumulator = JsonArgumentAccumulator::new();
        let mut ext = ExtensionStorage::new();

        let chunk = StreamEvent::ToolArgumentChunk {
            path: brioche_core::ExecutionPath::default(),
            id: "tc1".into(),
            chunk: From::from(&b"{}"[..]),
        };
        let _ = accumulator.on_stream_event(&chunk, &mut ext);

        let done = StreamEvent::ToolCallDone {
            path: brioche_core::ExecutionPath::default(),
        };
        let action = accumulator.on_stream_event(&done, &mut ext)?;

        assert!(matches!(action, StreamAction::Pass));
        let state = ext.get_or_insert_default::<JsonArgumentAccumulatorState>();
        assert!(state.accumulated.is_empty());
        Ok(())
    }
}
