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
///
/// Refs: I-Gov-Failover
pub struct SystemFailoverGuard;

impl SystemFailoverGuard {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
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

        Ok(Some(vec![
            Effect::ForwardToUi(brioche_core::UiWidget::CriticalError {
                component: plugin_name.0,
                detail: Some("governance component failed; system degraded".into()),
            }),
            Effect::SaveSession,
            Effect::SystemIdle,
        ]))
    }
}
