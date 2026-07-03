//! Timeout policies — Book II §5.14.
//!
//! - `ToolTimeoutPolicy`: bounds tool timeouts before `ExecuteTools` emission.
//! - `SubRoutineTimeoutPolicy`: limits sub-routine lifetime via deterministic ticks.
//!
//! Refs: I-Gov-Timeout-Bound, I-Gov-SubRoutineLifecycle-Guard

use std::collections::BTreeMap;

use brioche_core::{
    AgentStateTag, BriochePlugin, EngineInput, ExtensionStorage, PluginCapabilities, PluginResult,
    PolicyDecision, SessionSnapshot, ToolCallDescriptor,
};

use crate::Priority;

// ---------------------------------------------------------------------------
// ToolTimeoutPolicy
// ---------------------------------------------------------------------------

/// Timeout policy for tool calls.
///
/// On `on_tool_calls`, applies `default_timeout_ms` if absent and
/// caps to `max_timeout_ms` if defined.
///
/// Config is stored directly on the plugin; no separate state type is
/// needed because the values are immutable after construction.
///
/// Refs: I-Core-ActiveToolCall
pub struct ToolTimeoutPolicy {
    default_timeout_ms: u64,
    max_timeout_ms: u64,
}

impl ToolTimeoutPolicy {
    /// Creates a policy with a default timeout.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_default_timeout(default_timeout_ms: u64) -> Self {
        Self {
            default_timeout_ms,
            max_timeout_ms: 0,
        }
    }

    /// Creates a policy with a default timeout and a max cap.
    /// Refs: I-Gov-TraitAtomic
    pub fn with_bounds(default_timeout_ms: u64, max_timeout_ms: u64) -> Self {
        Self {
            default_timeout_ms,
            max_timeout_ms,
        }
    }
}

impl Default for ToolTimeoutPolicy {
    fn default() -> Self {
        Self::with_default_timeout(30000)
    }
}

impl BriochePlugin for ToolTimeoutPolicy {
    fn name(&self) -> &'static str {
        "tool_timeout_policy"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_TOOL_CALLS
    }

    fn priority(&self) -> i16 {
        Priority::TOOL_TIMEOUT
    }

    /// Applies the default timeout and caps to `max_timeout_ms`.
    ///
    /// # Complexity
    /// O(c). `c` calls; linear loop.
    fn on_tool_calls(
        &self,
        calls: &mut Vec<ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        for call in calls {
            let mut timeout = match call.timeout_ms {
                Some(t) => t,
                None => self.default_timeout_ms,
            };

            if self.max_timeout_ms > 0 && timeout > self.max_timeout_ms {
                timeout = self.max_timeout_ms;
            }

            call.timeout_ms = Some(timeout);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SubRoutineTimeoutPolicy
// ---------------------------------------------------------------------------

/// Sub-routine timer state.
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with number of active sub-routines
/// (typically < 5). One `BTreeMap` plus one scalar.
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
    /// Latest observed tick elapsed_ms from the shell's `TickEmitter`.
    pub last_tick_ms: u64,
    /// Map handle -> (start_tick_ms, timeout_limit_ms).
    pub timers: BTreeMap<brioche_core::SubRoutineHandle, (u64, u64)>,
}

/// Sub-routine timeout policy.
///
/// On `on_input`, verifies if any active sub-routine has exceeded its
/// timeout limit stored in `SubRoutineTimerState`.
/// Refs: I-Gov-TraitAtomic
///
/// Refs: I-Gov-SubRoutineLifecycle-Guard, I-Comp-Pure-Logic
pub struct SubRoutineTimeoutPolicy;

impl SubRoutineTimeoutPolicy {
    /// Creates a new policy instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }

    /// Creates a policy with a default timeout (API compatibility).
    ///
    /// Refs: I-Gov-TraitAtomic
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
        Priority::SUBROUTINE_TIMEOUT
    }

    /// Checks active sub-routine timers for expiry.
    ///
    /// Time is sourced deterministically from `SystemSignal::Tick`
    /// events stored in `SignalBuffer`, never from direct system time.
    ///
    /// # Complexity
    /// O(n) where n = number of tracked timers. Linear scan.
    fn on_input(
        &self,
        _input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let is_subroutine = {
            let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
            snapshot.current_state == AgentStateTag::SubRoutine
        };

        if !is_subroutine {
            let state = ext.get_or_insert_default::<SubRoutineTimerState>();
            state.timers.clear();
            return Ok(PolicyDecision::Allow);
        }

        let latest_tick = {
            let signal_buffer = ext.get_or_insert_default::<brioche_core::SignalBuffer>();
            signal_buffer
                .system_signals
                .iter()
                .filter_map(|s| match s {
                    brioche_core::SystemSignal::Tick { elapsed_ms } => Some(*elapsed_ms),
                    _ => None,
                })
                .next_back()
        };

        let state = ext.get_or_insert_default::<SubRoutineTimerState>();

        if let Some(tick) = latest_tick {
            state.last_tick_ms = tick;
        }

        let reference_ms = state.last_tick_ms;

        let expired: Vec<_> = state
            .timers
            .iter()
            .filter(|(_, (start, limit))| reference_ms.saturating_sub(*start) > *limit)
            .map(|(handle, _)| handle.clone())
            .collect();

        if let Some(handle) = expired.into_iter().next() {
            state.timers.remove(&handle);
            return Ok(PolicyDecision::Block {
                reason: format!("sub-routine {:?} exceeded timeout", handle),
            });
        }

        Ok(PolicyDecision::Allow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brioche_core::{
        AgentStateTag, EngineInput, ExtensionStorage, SessionSnapshot, SignalBuffer,
        SubRoutineHandle, SystemSignal, ToolCallDescriptor,
    };

    #[test]
    fn tool_timeout_policy_applies_default_when_missing() {
        let policy = ToolTimeoutPolicy::with_default_timeout(15000);
        let mut ext = ExtensionStorage::new();
        let mut calls = vec![ToolCallDescriptor {
            tool_id: "t1".into(),
            tool_name: "calc".into(),
            arguments: "{}".into(),
            timeout_ms: None,
        }];

        let result = policy.on_tool_calls(&mut calls, &mut ext);
        assert!(result.is_ok());
        assert_eq!(calls[0].timeout_ms, Some(15000));
    }

    #[test]
    fn tool_timeout_policy_caps_to_max() {
        let policy = ToolTimeoutPolicy::with_bounds(10000, 20000);
        let mut ext = ExtensionStorage::new();
        let mut calls = vec![ToolCallDescriptor {
            tool_id: "t1".into(),
            tool_name: "calc".into(),
            arguments: "{}".into(),
            timeout_ms: Some(50000),
        }];

        let result = policy.on_tool_calls(&mut calls, &mut ext);
        assert!(result.is_ok());
        assert_eq!(calls[0].timeout_ms, Some(20000));
    }

    fn subroutine_snapshot(ext: &mut ExtensionStorage) {
        let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
        snapshot.current_state = AgentStateTag::SubRoutine;
    }

    fn tick_at(ext: &mut ExtensionStorage, elapsed_ms: u64) {
        let buffer = ext.get_or_insert_default::<SignalBuffer>();
        buffer
            .system_signals
            .push(SystemSignal::Tick { elapsed_ms });
    }

    #[test]
    fn subroutine_timeout_policy_flags_expired_timer() {
        let policy = SubRoutineTimeoutPolicy::new();
        let mut ext = ExtensionStorage::new();
        let handle = SubRoutineHandle::new("sub");

        subroutine_snapshot(&mut ext);
        tick_at(&mut ext, 101);
        {
            let state = ext.get_or_insert_default::<SubRoutineTimerState>();
            state.last_tick_ms = 0;
            state.timers.insert(handle.clone(), (0, 100));
        }

        let decision = match policy.on_input(&EngineInput::UserMessage("tick".into()), &mut ext) {
            Ok(d) => d,
            Err(_) => {
                assert!(false, "on_input should succeed");
                return;
            }
        };

        assert!(
            matches!(decision, PolicyDecision::Block { .. }),
            "expired sub-routine should be blocked"
        );

        let state = ext.get_or_insert_default::<SubRoutineTimerState>();
        assert!(!state.timers.contains_key(&handle));
        assert_eq!(state.last_tick_ms, 101);
    }

    #[test]
    fn subroutine_timeout_policy_allows_active_timer() {
        let policy = SubRoutineTimeoutPolicy::new();
        let mut ext = ExtensionStorage::new();
        let handle = SubRoutineHandle::new("sub");

        subroutine_snapshot(&mut ext);
        tick_at(&mut ext, 50);
        {
            let state = ext.get_or_insert_default::<SubRoutineTimerState>();
            state.timers.insert(handle.clone(), (0, 100));
        }

        let decision = match policy.on_input(&EngineInput::UserMessage("tick".into()), &mut ext) {
            Ok(d) => d,
            Err(_) => {
                assert!(false, "on_input should succeed");
                return;
            }
        };

        assert!(matches!(decision, PolicyDecision::Allow));

        let state = ext.get_or_insert_default::<SubRoutineTimerState>();
        assert!(state.timers.contains_key(&handle));
    }

    #[test]
    fn subroutine_timeout_policy_clears_timers_outside_subroutine() {
        let policy = SubRoutineTimeoutPolicy::new();
        let mut ext = ExtensionStorage::new();
        let handle = SubRoutineHandle::new("sub");

        {
            let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
            snapshot.current_state = AgentStateTag::Idle;
            let state = ext.get_or_insert_default::<SubRoutineTimerState>();
            state.timers.insert(handle.clone(), (0, 100));
        }

        let decision = match policy.on_input(&EngineInput::UserMessage("exit".into()), &mut ext) {
            Ok(d) => d,
            Err(_) => {
                assert!(false, "on_input should succeed");
                return;
            }
        };

        assert!(matches!(decision, PolicyDecision::Allow));

        let state = ext.get_or_insert_default::<SubRoutineTimerState>();
        assert!(state.timers.is_empty());
    }
}
