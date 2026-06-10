//! Book I — The Core Book: Plugin interface and governance trait contracts.
//!
//! This module defines:
//! - `BriochePlugin`: the core plugin trait with capability-based routing.
//! - `PluginCapabilities`: bitmask for O(1) route pre-computation.
//! - Governance anchor traits called sequentially by `BriocheEngine::transition()`.
//!
//! Invariants upheld:
//! - I-Core-PluginOrder: Total order via `priority` + `name`.
//! - I-Core-StreamNoBranch: Pre-routed `UnifiedRoutingTable` eliminates hot-path branching.
//! - I-Gov-TraitAtomic: Each trait is a standalone capability.
//!
//! Refs: SPECS.md §4, §Book II

use std::any::{Any, TypeId};

use crate::{
    ChatMessage, Effect, EngineInput, ExtVTable, ExtensionStorage, PluginError, PluginResult,
    PolicyDecision, Session, SessionRegistry, StreamAction, StreamEvent, SubRoutineHandle,
    ToolCallDescriptor, ToolResultDTO,
};

// ---------------------------------------------------------------------------
// PluginCapabilities
// ---------------------------------------------------------------------------

/// Bitmask of plugin hook subscriptions.
///
/// Plugins declare their capabilities via this bitmask. At engine
/// initialization, the `UnifiedRoutingTable` pre-computes routes for
/// each capability, eliminating runtime mask checks in the hot path.
///
/// Refs: I-Core-StreamNoBranch, I-Core-PluginOrder
/// # Complexity
/// O(1) for construction and field/variant access.
/// # Panics
/// Never panics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PluginCapabilities(pub u16);

impl PluginCapabilities {
    /// Capability bit for the `after_prediction` hook.
    pub const AFTER_PREDICTION: Self = Self(1 << 3);
    /// Capability bit for the `before_prediction` hook.
    pub const BEFORE_PREDICTION: Self = Self(1 << 1);
    /// No hooks subscribed.
    pub const NONE: Self = Self(0);
    /// Capability bit for the `on_error` hook.
    pub const ON_ERROR: Self = Self(1 << 6);
    /// Capability bit for the `on_input` hook.
    pub const ON_INPUT: Self = Self(1 << 0);
    /// Capability bit for the `on_stream_event` hook.
    pub const ON_STREAM_EVENT: Self = Self(1 << 2);
    /// Capability bit for the `on_tool_calls` hook.
    pub const ON_TOOL_CALLS: Self = Self(1 << 4);
    /// Capability bit for the `on_tool_result` hook.
    pub const ON_TOOL_RESULT: Self = Self(1 << 5);

    /// Returns `true` if this capability set includes `other`.
    ///
    /// Complexity: O(1). Bitwise AND.
    ///
    /// Refs: I-Core-Pure
    /// # Panics
    /// Never panics.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Returns `true` if no capabilities are set.
    ///
    /// Complexity: O(1). Equality check.
    ///
    /// Refs: I-Core-Pure
    /// # Panics
    /// Never panics.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Combine two capability sets.
///
/// Used by plugins to declare multiple hook subscriptions.
///
/// Refs: I-Core-StreamNoBranch
impl std::ops::BitOr for PluginCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

// ---------------------------------------------------------------------------
// BriochePlugin
// ---------------------------------------------------------------------------

/// Core plugin trait — all policy lives in plugins, never in the kernel.
///
/// Each plugin declares its capabilities via `PluginCapabilities`. The
/// kernel pre-routes plugins into `UnifiedRoutingTable` at initialization,
/// guaranteeing O(1) dispatch in the streaming hot path.
///
/// All hooks have default implementations returning "allow/pass/ok",
/// so a plugin only needs to override the hooks it cares about.
///
/// Refs: I-Core-PluginOrder, I-Gov-NoCoreMutation
/// # Panics
/// Panics only if an index is out of bounds; callers must validate lengths.
pub trait BriochePlugin: Send + Sync {
    /// Unique plugin name, used for total ordering and traceability.
    fn name(&self) -> &'static str;

    /// Hook subscriptions. Determines which routes the plugin is placed on.
    fn capabilities(&self) -> PluginCapabilities;

    /// Deterministic evaluation order. Lower priority = evaluated first.
    /// Ties are broken by `name` lexicographically.
    fn priority(&self) -> i16 {
        0
    }

    /// Reserved keys in `ExtensionStorage`. Format: `"plugin_name::state_name"`.
    fn owned_state_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Serializes the default state into a binary blob for resilient storage.
    fn default_state_blob(&self) -> Vec<u8> {
        vec![]
    }

    /// Attempts to deserialize a blob. On failure, the engine calls
    /// `default_state_blob`.
    fn deserialize_state(&self, _raw: &[u8]) -> Result<Box<dyn Any + Send + Sync>, String> {
        Err("Not implemented".into())
    }

    /// Input interceptor hook. Allows a governance plugin to entirely
    /// replace the standard dispatch.
    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    /// Hook called before LLM prediction. Decisions are collected and
    /// passed to `DecisionAggregator`.
    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    /// Stream event hook — the hot path. Plugins return `StreamAction`
    /// rather than `PolicyDecision` to avoid branching in the streaming loop.
    fn on_stream_event(
        &self,
        _event: &StreamEvent,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        Ok(StreamAction::Pass)
    }

    /// Hook called after prediction completes (before tool execution).
    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }

    /// Hook called before emission of the `ExecuteTools` effect.
    /// Plugins mutate `timeout_ms` and other fields in place.
    fn on_tool_calls(
        &self,
        _calls: &mut Vec<ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }

    /// Hook called before persistence of tool results in history.
    /// Plugins may mutate results in place.
    fn on_tool_result(
        &self,
        _results: &mut Vec<ToolResultDTO>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }

    /// Hook called by the core when a plugin error is intercepted.
    fn on_error(
        &self,
        _error: &PluginError,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

// ---------------------------------------------------------------------------
// Governance traits — Book II anchor points
// ---------------------------------------------------------------------------

/// First governance trait evaluated in every transition cycle.
///
/// If `Block`, the kernel returns `Error` + `SystemIdle` immediately.
/// No subsequent trait may override an epoch barrier.
///
/// Refs: I-Comp-Epoch-First, I-Gov-Epoch-Reject
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait EpochInterceptor: Send + Sync {
    /// Intercept epoch.
    fn intercept_epoch(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<crate::EpochAction>;
}

/// Handles sub-routine delegation and resolution.
///
/// Called if `session.state` is `SubRoutine`. Resolves the child via
/// `SessionRegistry`. If `Some(effects)`, standard dispatch is short-circuited.
///
/// Refs: I-Comp-Epoch-Subroutine
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait SubRoutineHandler: Send + Sync {
    /// Handle subroutine.
    fn handle_subroutine(
        &self,
        parent: &mut Session,
        child: &mut Session,
        input: &EngineInput,
    ) -> PluginResult<Option<Vec<Effect>>>;
}

/// Post-transition mechanical consistency check.
///
/// Called last in the transition cycle. If `Some(effects)`, the kernel
/// applies mechanical forcing (typically `OverrideTransition` to `Idle`).
///
/// Refs: I-Core-NoPanic
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait ConsistencyVerifier: Send + Sync {
    /// Verify consistency.
    fn verify_consistency(&self, session: &mut Session) -> PluginResult<Option<Vec<Effect>>>;
}

/// Mandatory. Aggregates `before_prediction` decisions from multiple plugins.
///
/// The kernel refuses to start without an injected aggregator.
///
/// Refs: I-Gov-Decision-Required
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait DecisionAggregator: Send + Sync {
    /// Aggregate decisions.
    fn aggregate_decisions(
        &self,
        decisions: Vec<PolicyDecision>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision>;
}

/// Defines the invariant drainage order of separate channels.
///
/// The shell implements this trait and the engine thread loop calls
/// `drain()` before each `transition()` cycle. The returned batch is
/// injected into `ExtensionStorage` as `SignalBuffer` so that plugins
/// can consume pending signals in their hooks.
///
/// Canonical order: `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`.
///
/// Refs: SPECS.md §1.4, I-Shell-Drain-Atomic
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait SignalDrainOrder: Send + Sync {
    /// Drain.
    fn drain(&self) -> crate::SignalDrainBatch;
}

/// Optional. O(1) validation of effects requested by plugins on specific hooks.
///
/// Without injection, all `RequestEffect`s are allowed on all hooks.
///
/// Refs: I-Core-HookEffect-O1
/// # Panics
/// Never panics.
pub trait HookEffectConstraint: Send + Sync {
    /// O(1) validation by bitmask lookup.
    ///
    /// `hook_index`: compact hook index (0-7).
    /// `effect_mask`: bitmask of the effect to validate (`EffectBit` constant).
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;

    /// Validation by name (fallback for custom/future extensions).
    fn is_allowed_fallback(&self, hook_name: &str, effect_variant: &str) -> bool;
}

/// Optional. Granular COW rollback of `ExtensionStorage` on budget overrun.
///
/// Without injection, the kernel performs no snapshot or rollback.
///
/// Refs: I-Gov-Rollback-BestEffort
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait CycleRollbackPolicy: Send + Sync {
    /// Called by the kernel before each monitored hook.
    fn begin_hook(&mut self);

    /// Called by the kernel when an extension is mutated for the first time
    /// in this hook. The VTable `clone_box` provides the clone.
    fn on_mutation(&mut self, type_id: TypeId, vtable: &ExtVTable, current: &dyn Any);

    /// Called if the budget is respected — mutations are kept.
    fn commit_hook(&mut self, ext: &mut ExtensionStorage);

    /// Called if the budget is exceeded — restoration from snapshots.
    fn rollback_hook(&mut self, ext: &mut ExtensionStorage);
}

/// Mandatory. Cleanup of `SessionRegistry` on outgoing transition from `SubRoutine`.
///
/// The kernel refuses to start without an injected implementation.
///
/// Refs: I-Gov-SubRoutineLifecycle-Guard
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait SubRoutineLifecycleGuard: Send + Sync {
    /// On exit.
    fn on_exit(
        &self,
        handle: SubRoutineHandle,
        parent: &mut Session,
        registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>>;
}

/// Optional. Safety net in case of cascading governance trait failures.
///
/// Without injection, the kernel returns the raw `PluginFault`.
///
/// Refs: SPECS.md §2.10
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait GovernanceFailoverHandler: Send + Sync {
    /// Handle failure.
    fn handle_failure(
        &self,
        session: &mut Session,
        fault: &Effect,
    ) -> PluginResult<Option<Vec<Effect>>>;
}

/// Optional. Per-hook COW budget for `CycleRollbackPolicy` implementations.
///
/// Without injection, the default value is 65536 bytes (64 KB).
///
/// Refs: SPECS.md §2.11
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait CowBudgetPolicy: Send + Sync {
    /// Max cow bytes.
    fn max_cow_bytes(&self, hook_name: &str) -> usize;
}
