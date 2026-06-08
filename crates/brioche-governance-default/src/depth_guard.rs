//! DepthGuard — Book II §5.12.
//!
//! Limits sub-routine nesting depth via `DepthState`.
//!
//! Refs: I-Gov-Depth-Limit

use brioche_core::{
    AgentStateTag, BriochePlugin, Effect, EngineInput, ErrorCode, ExtensionStorage,
    PluginCapabilities, PluginResult, PolicyDecision, SessionSnapshot,
};

/// Nesting depth tracking state.
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
    /// Maximum allowed depth.
    pub max_depth: u64,
    /// Current depth (derived from `state_stack_depth` on the fly).
    pub current_depth: u64,
}

/// Garde de profondeur de sous-routines.
///
/// On `on_input`, verifies that the stack depth does not exceed
/// `max_depth`. If so, emits an `OverrideTransition` to
/// `Idle` with a UI notification.
pub struct DepthGuard {
    max_depth: u64,
}

impl DepthGuard {
    /// Creates a guard with a maximum depth.
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
        -40 // After epoch, recovery, but before business logic
    }

    /// Verifies depth before each `UserMessage`.
    ///
    /// # Complexity
    /// O(log n). Two `ExtensionStorage` reads (`SessionSnapshot`, `DepthState`).
    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        // Only check on user messages (new intentions that may create sub-routines).
        if !matches!(input, EngineInput::UserMessage(_)) {
            return Ok(PolicyDecision::Allow);
        }

        let current_depth = ext.with_or_insert_default::<SessionSnapshot, _>(|snapshot| {
            calculate_depth(snapshot.state_stack_depth, snapshot.current_state)
        });

        ext.with_or_insert_default::<DepthState, _>(|state| {
            state.max_depth = self.max_depth;
            state.current_depth = current_depth;
        });

        if current_depth >= self.max_depth {
            return Ok(PolicyDecision::OverrideTransition(vec![
                Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    message: format!(
                        "sub-routine depth limit exceeded: {} >= {}",
                        current_depth, self.max_depth
                    ),
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
