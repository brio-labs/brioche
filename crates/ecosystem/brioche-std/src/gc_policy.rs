//! GcPolicy — Book IV §1.7.
//!
//! Decides whether to trigger opportunistic garbage collection.
//! On `after_prediction`, if the session has reached `Idle` state
//! and a configurable number of cycles has passed since the last GC,
//! requests `TriggerGc`.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use brioche_core::{
    AgentStateTag, BriocheExtensionType, BriochePlugin, ExtensionStorage, PluginCapabilities,
    PluginResult, SessionSnapshot,
};

/// GC policy state.
///
/// ## Snapshot strategy
/// COW: full clone (~32 bytes). Four scalar fields.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct GcPolicyState {
    /// Trigger GC every N prediction cycles (0 = disabled).
    pub cycle_interval: u64,
    /// Cycles since last GC.
    pub cycles_since_gc: u64,
    /// Total number of GCs triggered.
    pub gcs_triggered: u64,
    /// Whether to trigger GC only when transitioning to Idle.
    pub only_when_idle: bool,
}

/// GC policy plugin.
///
/// Requests `TriggerGc` based on cycle count and idle state.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct GcPolicy {
    cycle_interval: u64,
    only_when_idle: bool,
}

impl GcPolicy {
    /// Creates a policy with a cycle interval.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_cycle_interval(cycle_interval: u64) -> Self {
        Self {
            cycle_interval,
            only_when_idle: true,
        }
    }

    /// Creates a policy that triggers unconditionally every N cycles.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_unconditional_interval(cycle_interval: u64) -> Self {
        Self {
            cycle_interval,
            only_when_idle: false,
        }
    }
}

impl Default for GcPolicy {
    fn default() -> Self {
        Self::with_cycle_interval(10)
    }
}

impl BriochePlugin for GcPolicy {
    fn name(&self) -> &'static str {
        "gc_policy"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::AFTER_PREDICTION
    }

    fn priority(&self) -> i16 {
        200 // Very late observer — only trigger GC after all other processing
    }

    /// Triggers GC if cycle threshold is met.
    ///
    /// # Complexity
    /// O(1). Two ExtensionStorage reads.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        // Read snapshot first so the mutable borrow ends before state access.
        let is_idle = {
            let snapshot = ext.get_or_insert_default::<SessionSnapshot>();
            snapshot.current_state == AgentStateTag::Idle
        };

        let state = ext.get_or_insert_default::<GcPolicyState>();
        state.cycle_interval = self.cycle_interval;
        state.only_when_idle = self.only_when_idle;
        state.cycles_since_gc += 1;

        if self.cycle_interval == 0 {
            return Ok(());
        }

        if state.cycles_since_gc >= self.cycle_interval && (!self.only_when_idle || is_idle) {
            state.cycles_since_gc = 0;
            state.gcs_triggered += 1;
            // GC is requested as a side-effect via a mechanism not directly
            // available in after_prediction (which returns PluginResult<()>).
            // In a full shell integration, the plugin would emit a telemetry
            // event or the shell would poll GcPolicyState.
            // For Sprint 16, we update state only; the shell checks
            // GcPolicyState.gcs_triggered to decide when to emit TriggerGc.
        }

        Ok(())
    }
}
