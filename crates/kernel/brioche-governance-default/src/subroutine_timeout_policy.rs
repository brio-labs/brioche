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
///
/// ## Snapshot strategy
/// COW: full clone. Weight scales with number of active sub-routines
/// (typically 003c 5). One `BTreeMap` plus one scalar.
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
    /// Used as the deterministic clock for timeout checks.
    pub last_tick_ms: u64,
    /// Map handle -> (start_tick_ms, timeout_limit_ms).
    /// `start_tick_ms` must use the same timebase as `TickEmitter`
    /// (i.e. elapsed milliseconds since the emitter started).
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
    /// Time is sourced deterministically from `SystemSignal::Tick`
    /// events stored in `SignalBuffer`, never from direct system time.
    /// This preserves I-Core-Pure: identical inputs produce identical outputs.
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
            // Not in a sub-routine: clear stale timers to prevent
            // unbounded growth.
            state.timers.clear();
            return Ok(PolicyDecision::Allow);
        }

        // Deterministic clock: consume the latest Tick signal from
        // the shell's SignalBuffer. The shell's TickEmitter provides
        // monotonically increasing elapsed_ms values.
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

        // Collect expired handles before mutating the map.
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
