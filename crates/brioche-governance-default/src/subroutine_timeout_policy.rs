//! SubRoutineTimeoutPolicy — Book II §5.14.
//!
//! Consumes `SystemSignal::Tick` (via shell adapter) and limits
//! sub-routine lifetime. In the current architecture, this plugin
//! registers state and would be triggered by a shell-side adapter
//! that drains tick signals into the engine.
//!
//! Refs: I-Gov-SubRoutineLifecycle-Guard

use brioche_core::{
    AgentStateTag, BriochePlugin, EngineInput, ExtensionStorage, PluginCapabilities, PluginResult,
    PolicyDecision, SessionSnapshot,
};
use std::collections::BTreeMap;

/// Sub-routine timer state.
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
pub struct SubRoutineTimerState {
    /// Map handle -> (timestamp_start_ms, timeout_limit_ms).
    pub timers: BTreeMap<brioche_core::SubRoutineHandle, (u64, u64)>,
}

/// Sub-routine timeout policy.
///
/// On `on_input`, verifies if any active sub-routine has exceeded its
/// timeout limit stored in `SubRoutineTimerState`. Timers are populated
/// by the shell-side adapter (Sprint 9+); this plugin performs the
/// policy check only.
///
/// Refs: I-Gov-SubRoutineLifecycle-Guard, I-Comp-Pure-Logic
pub struct SubRoutineTimeoutPolicy;

impl SubRoutineTimeoutPolicy {
    /// Creates a new policy instance.
    pub fn new() -> Self {
        Self
    }

    /// Creates a policy with a default timeout (ignored until shell
    /// adapter populates timers; kept for API compatibility).
    pub fn with_default_timeout(_default_timeout_ms: u64) -> Self {
        Self::new()
    }
}

impl Default for SubRoutineTimeoutPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for SubRoutineTimeoutPolicy {
    fn name(&self) -> &'static str {
        "subroutine_timeout_policy"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        -30 // After epoch, recovery, depth
    }

    /// Checks active sub-routine timers for expiry.
    ///
    /// # Complexity
    /// O(n) where n = number of tracked timers. Linear scan.
    fn on_input(
        &self,
        _input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let is_subroutine = ext.with_or_insert_default::<SessionSnapshot, _>(|snapshot| {
            snapshot.current_state == AgentStateTag::SubRoutine
        });

        if !is_subroutine {
            // Not in a sub-routine: clear stale timers to prevent
            // unbounded growth.
            ext.with_or_insert_default::<SubRoutineTimerState, _>(|state| {
                state.timers.clear();
            });
            return Ok(PolicyDecision::Allow);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        ext.with_or_insert_default::<SubRoutineTimerState, _>(|state| {
            // Collect expired handles before mutating the map.
            let expired: Vec<_> = state
                .timers
                .iter()
                .filter(|(_, (start, limit))| now.saturating_sub(*start) > *limit)
                .map(|(handle, _)| handle.clone())
                .collect();

            if let Some(handle) = expired.into_iter().next() {
                state.timers.remove(&handle);
                return Ok(PolicyDecision::Block {
                    reason: format!("sub-routine {:?} exceeded timeout", handle),
                });
            }

            Ok(PolicyDecision::Allow)
        })
    }
}
