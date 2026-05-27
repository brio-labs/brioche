//! DepthGuard — Book II §5.12.
//!
//! Limite la profondeur d'imbrication des sous-routines via `DepthState`.
//!
//! Refs: I-Gov-Depth-Limit

use brioche_core::{
    AgentStateTag, BriochePlugin, Effect, EngineInput, ErrorCode, ExtensionStorage,
    PluginCapabilities, PluginResult, PolicyDecision, SessionSnapshot,
};

/// État de suivi de la profondeur d'imbrication.
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
    /// Profondeur maximale autorisée.
    pub max_depth: u64,
    /// Profondeur actuelle (dérivée de `state_stack_depth` à la volée).
    pub current_depth: u64,
}

/// Garde de profondeur de sous-routines.
///
/// Sur `on_input`, vérifie que la profondeur de pile n'excède pas
/// `max_depth`. Si c'est le cas, émet un `OverrideTransition` vers
/// `Idle` avec une notification UI.
pub struct DepthGuard {
    max_depth: u64,
}

impl DepthGuard {
    /// Crée un garde avec une profondeur maximale.
    pub fn with_max_depth(max_depth: u64) -> Self {
        Self { max_depth }
    }
}

impl Default for DepthGuard {
    fn default() -> Self {
        Self::with_max_depth(10)
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

    /// Vérifie la profondeur avant chaque `UserMessage`.
    ///
    /// # Complexity
    /// O(log n). Deux lectures `ExtensionStorage` (`SessionSnapshot`, `DepthState`).
    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        // Only check on user messages (new intentions that may create sub-routines).
        if !matches!(input, EngineInput::UserMessage(_)) {
            return Ok(PolicyDecision::Allow);
        }

        let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
        let stack_depth = snapshot.state_stack_depth as u64;

        // SubRoutine state itself contributes +1 to perceived depth.
        let current_depth = if snapshot.current_state == AgentStateTag::SubRoutine {
            stack_depth + 1
        } else {
            stack_depth
        };

        let state = ext.get_or_insert_default::<DepthState>();
        state.max_depth = self.max_depth;
        state.current_depth = current_depth;

        if current_depth >= self.max_depth {
            return Ok(PolicyDecision::OverrideTransition(vec![
                Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    message: format!(
                        "sub-routine depth limit exceeded: {} >= {}",
                        current_depth, self.max_depth
                    ),
                },
                Effect::ForwardToUi {
                    widget_type: "error".into(),
                    payload: {
                        let mut map = serde_json::Map::new();
                        map.insert(
                            "code".to_string(),
                            serde_json::Value::String("DEPTH_LIMIT_EXCEEDED".into()),
                        );
                        map.insert(
                            "depth".to_string(),
                            serde_json::Value::Number(current_depth.into()),
                        );
                        map.insert(
                            "max".to_string(),
                            serde_json::Value::Number(self.max_depth.into()),
                        );
                        serde_json::Value::Object(map)
                    },
                },
                Effect::SystemIdle,
            ]));
        }

        Ok(PolicyDecision::Allow)
    }
}
