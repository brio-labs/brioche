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
