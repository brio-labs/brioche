//! SystemFailoverGuard — `GovernanceFailoverHandler` implementation (Book II §5.19).
//!
//! Safety net in case of cascading failure of a governance plugin.
//! Forces a safe terminal state (`Idle` + UI notification).
//!
//! Refs: SPECS.md §2.10

use brioche_core::{Effect, GovernanceFailoverHandler, PluginResult, Session};

/// System failover guard.
///
/// Intercepts `Effect::PluginFault` emanating from fundamental plugins
/// and replaces the effect sequence with a safe terminal state.
pub struct SystemFailoverGuard;

impl SystemFailoverGuard {
    /// Creates a new instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemFailoverGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl GovernanceFailoverHandler for SystemFailoverGuard {
    fn handle_failure(
        &self,
        _session: &mut Session,
        fault: &Effect,
    ) -> PluginResult<Option<Vec<Effect>>> {
        let plugin_name = match fault {
            Effect::PluginFault { plugin_name, .. } => plugin_name.clone(),
            _ => return Ok(None),
        };

        let mut payload = serde_json::Map::new();
        payload.insert(
            "component".to_string(),
            serde_json::Value::String(plugin_name),
        );
        payload.insert(
            "message".to_string(),
            serde_json::Value::String("governance component failed; system degraded".into()),
        );

        Ok(Some(vec![
            Effect::ForwardToUi {
                widget_type: "critical_error".into(),
                payload: serde_json::Value::Object(payload),
            },
            Effect::SaveSession,
            Effect::SystemIdle,
        ]))
    }
}
