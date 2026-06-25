//! Observability and rollback trace types.
//!
//! Transition traces, rollback event logs, and superseded transition records.

use serde::{Deserialize, Serialize};

use super::effect::PolicyDecision;
use crate::BriocheExtensionType;

/// Event log for COW rollback telemetry.
///
/// Written by `CycleRollbackPolicy` implementations during `commit_hook`
/// and `rollback_hook`, then consumed by `RollbackTelemetryEmitter`.
///
/// ## Snapshot strategy
/// COW: full clone. Weight ~O(n) where n = events (typically < 10).
///
/// Refs: I-Gov-Rollback-BestEffort, I-Comp-Pure-Logic
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct RollbackEventLog {
    /// Events recorded since the last consumption.
    #[brioche(deterministic_order)]
    pub events: Vec<RollbackEvent>,
}

/// Single COW rollback event.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Gov-Rollback-BestEffort
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RollbackEvent {
    /// Hook name during which the event occurred.
    pub hook_name: String,
    /// `true` = rollback restored snapshots; `false` = commit discarded them.
    pub was_rollback: bool,
    /// Cumulative weight of the frame at decision time (bytes).
    pub frame_weight: usize,
    /// Whether the budget was exceeded (abandoned rollback).
    pub budget_exceeded: bool,
}

// Trace types (for OverrideTransition traceability)
// ---------------------------------------------------------------------------

/// Single entry in the `TransitionTraceLog` ring buffer.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionTrace {
    /// Name of the plugin that emitted the `OverrideTransition`.
    pub source_plugin: String,
    /// The actual decision that was applied.
    pub decision: PolicyDecision,
    /// Generation ID at the time of the override.
    pub epoch: u64,
}

/// Ring buffer for traceability of applied `OverrideTransition`s (max 128 entries, FIFO).
///
/// ## Snapshot strategy
/// COW: full clone. Weight ~O(n) where n = entries (max 128).
///
/// Refs: I-Gov-OverrideTrace, I-Core-NoPanic
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct TransitionTraceLog {
    #[brioche(deterministic_order)]
    /// Ring buffer of overrides (max 128, FIFO eviction).
    pub entries: Vec<TransitionTrace>,
}

impl TransitionTraceLog {
    const CAPACITY: usize = 128;

    /// Push a trace entry, evicting the oldest if at capacity.
    ///
    /// # Complexity
    /// O(n) in the worst case (vec shift at capacity), bounded by
    /// `CAPACITY` (128).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn push(&mut self, entry: TransitionTrace) {
        if self.entries.len() >= Self::CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// Take all entries, leaving the log empty.
    ///
    /// # Complexity
    /// O(1).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn take_entries(&mut self) -> Vec<TransitionTrace> {
        std::mem::take(&mut self.entries)
    }
}

/// Single entry in the `SupersededTransitionTraceLog` ring buffer.
///
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
/// Refs: I-Gov-OverrideTrace
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupersededTransitionTrace {
    /// Name of the plugin that emitted the `OverrideTransition`.
    pub source_plugin: String,
    /// The decision that was overridden.
    pub attempted_decision: PolicyDecision,
    /// Name of the plugin whose override won.
    pub preempted_by: String,
    /// Generation ID at the time of the override.
    pub epoch: u64,
}

/// Ring buffer of preempted `OverrideTransition`s (max 128 entries, FIFO).
///
/// ## Snapshot strategy
/// COW: full clone. Weight ~O(n) where n = entries (max 128).
///
/// Refs: I-Gov-OverrideTrace
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct SupersededTransitionTraceLog {
    #[brioche(deterministic_order)]
    /// Ring buffer of overrides (max 128, FIFO eviction).
    pub entries: Vec<SupersededTransitionTrace>,
}

impl SupersededTransitionTraceLog {
    const CAPACITY: usize = 128;

    /// Push a trace entry, evicting the oldest if at capacity.
    ///
    /// # Complexity
    /// O(n) in the worst case (vec shift at capacity), bounded by
    /// `CAPACITY` (128).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn push(&mut self, entry: SupersededTransitionTrace) {
        if self.entries.len() >= Self::CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    /// Take all entries, leaving the log empty.
    ///
    /// # Complexity
    /// O(1).
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-OverrideTrace
    pub fn take_entries(&mut self) -> Vec<SupersededTransitionTrace> {
        std::mem::take(&mut self.entries)
    }
}
