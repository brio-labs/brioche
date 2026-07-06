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
//! Refs: docs/SPECS.md §4, §Book II

use std::any::{Any, TypeId};

use crate::{
    BriocheError, ChatMessage, Effect, EngineInput, ExtVTable, ExtensionStorage, PluginError,
    PluginResult, PolicyDecision, Session, SessionRegistry, StreamAction, StreamEvent,
    SubRoutineHandle, ToolCallDescriptor, ToolResultDTO,
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
    ///
    /// Must be globally unique within an engine. The name is used as a
    /// tie-breaker when two plugins share the same `priority`.
    ///
    /// # Complexity
    /// O(1). Returns a static string.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-PluginOrder
    fn name(&self) -> &'static str;

    /// Hook subscriptions. Determines which routes the plugin is placed on.
    ///
    /// The kernel pre-computes a `UnifiedRoutingTable` from these masks at
    /// engine construction time. The hot path then uses the route index,
    /// never the bitmask.
    ///
    /// # Complexity
    /// O(1). Returns a bitmask.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-StreamNoBranch
    fn capabilities(&self) -> PluginCapabilities;

    /// Deterministic evaluation order. Lower priority = evaluated first.
    ///
    /// Ties are broken by `name` lexicographically. Use named constants
    /// (e.g. `Priority::DepthGuard`) instead of magic numbers.
    ///
    /// # Complexity
    /// O(1). Returns a scalar.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-PluginOrder
    fn priority(&self) -> i16 {
        0
    }

    /// Reserved keys in `ExtensionStorage`. Format: `"plugin_name::state_name"`.
    ///
    /// Returning owned keys allows the engine to snapshot/restore only the
    /// state belonging to this plugin. An empty slice means the plugin
    /// stores no persisted state.
    ///
    /// # Complexity
    /// O(1). Returns a static slice.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-ExtO1
    fn owned_state_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Serializes the default state into a binary blob for resilient storage.
    ///
    /// Used by persistence layers when no prior state exists. The default
    /// implementation returns an empty blob; plugins with state should
    /// override this.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation.
    ///
    /// Refs: I-Persist-Idempotence
    fn default_state_blob(&self) -> Vec<u8> {
        vec![]
    }

    /// Attempts to deserialize a blob. On failure, the engine calls
    /// `default_state_blob`.
    ///
    /// The default implementation always fails, which is appropriate for
    /// stateless plugins.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation.
    ///
    /// Refs: I-Persist-Idempotence
    fn deserialize_state(&self, _raw: &[u8]) -> Result<Box<dyn Any + Send + Sync>, String> {
        Err("Not implemented".into())
    }

    /// Input interceptor hook. Allows a governance plugin to entirely
    /// replace the standard dispatch.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation. Plugin implementations
    /// must uphold the NoPanic invariant.
    ///
    /// Refs: I-Core-NoPanic
    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    /// Hook called before LLM prediction. Decisions are collected and
    /// passed to `DecisionAggregator`.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation. Plugin implementations
    /// must uphold the NoPanic invariant.
    ///
    /// Refs: I-Core-NoPanic
    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    /// Stream event hook — the hot path. Plugins return `StreamAction`
    /// rather than `PolicyDecision` to avoid branching in the streaming loop.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation. Plugin implementations
    /// must uphold the NoPanic invariant.
    ///
    /// Refs: I-Core-NoPanic
    fn on_stream_event(
        &self,
        _event: &StreamEvent,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<StreamAction> {
        Ok(StreamAction::Pass)
    }

    /// Hook called after prediction completes (before tool execution).
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation. Plugin implementations
    /// must uphold the NoPanic invariant.
    ///
    /// Refs: I-Core-NoPanic
    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }

    /// Hook called before emission of the `ExecuteTools` effect.
    /// Plugins mutate `timeout_ms` and other fields in place.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation. Plugin implementations
    /// must uphold the NoPanic invariant.
    ///
    /// Refs: I-Core-NoPanic
    fn on_tool_calls(
        &self,
        _calls: &mut Vec<ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }

    /// Hook called before persistence of tool results in history.
    /// Plugins may mutate results in place.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation. Plugin implementations
    /// must uphold the NoPanic invariant.
    ///
    /// Refs: I-Core-NoPanic
    fn on_tool_result(
        &self,
        _results: &mut Vec<ToolResultDTO>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        Ok(())
    }

    /// Hook called by the core when a plugin error is intercepted.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    ///
    /// # Panics
    /// Never panics in the default implementation. Plugin implementations
    /// must uphold the NoPanic invariant.
    ///
    /// Refs: I-Core-NoPanic
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
    /// Intercept the current epoch to enforce temporal isolation.
    ///
    /// Compares the `generation_id` carried by `input` against the
    /// current epoch stored in `ExtensionStorage`. Returns
    /// `EpochAction::Block` if the input belongs to a stale epoch.
    ///
    /// # Complexity
    /// O(1). One `ExtensionStorage` read plus a scalar comparison.
    ///
    /// # Panics
    /// Never panics. Implementations must return `PluginError` rather
    /// than panic.
    ///
    /// Refs: I-Comp-Epoch-First, I-Gov-Epoch-Reject
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
    /// Delegate or resolve a sub-routine transition.
    ///
    /// Called when the parent session is in `AgentState::SubRoutine`.
    /// The handler may read `parent`, mutate `child`, and optionally
    /// return effects that short-circuit standard dispatch.
    ///
    /// # Complexity
    /// O(1) for the trait call. Implementations may incur lookup cost.
    ///
    /// # Panics
    /// Never panics. Implementations must return `PluginError` rather
    /// than panic.
    ///
    /// Refs: I-Comp-Epoch-Subroutine
    fn handle_subroutine(
        &self,
        parent: &mut Session,
        child: &mut Session,
        input: &EngineInput,
    ) -> PluginResult<Option<Vec<Effect>>>;
}

/// Hydrates a sub-routine session from a persisted MessagePack head blob.
///
/// This is a boundary trait: `brioche-core` defines the contract, while
/// `brioche-shell-persistence` supplies the MessagePack implementation.
/// Keeping the trait in Core preserves the dependency direction
/// (persistence depends on core, never the reverse).
///
/// # Complexity
/// O(deserialization cost). The trait itself introduces no allocation.
///
/// # Panics
/// Never panics. Implementations must return `BriocheError` on failure.
///
/// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
pub trait SubRoutineHydrator: Send + Sync {
    /// Deserialize `head_blob` into a `Session`.
    ///
    /// The returned `Session` is created in memory and can be inserted
    /// into the `SessionRegistry` by the kernel.
    ///
    /// # Complexity
    /// O(deserialization cost + session reconstruction).
    ///
    /// # Errors
    /// Returns `BriocheError::Serialization` or another deterministic error
    /// if the blob cannot be decoded.
    fn hydrate(&self, head_blob: &[u8]) -> Result<Session, BriocheError>;
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
    /// Verify mechanical consistency of `session` after a transition.
    ///
    /// Implementations must **not** mutate `session`. If the state is
    /// inconsistent, return `PolicyDecision::OverrideTransition(effects)`
    /// and the kernel will apply the standard recovery (transition to
    /// `Idle`, clear the state stack, and clear active tools) before
    /// appending the returned effects.
    ///
    /// # Complexity
    /// O(1) for the trait call. Implementations must document their own
    /// complexity.
    ///
    /// # Panics
    /// Never panics. Implementations must return `PluginError` rather
    /// than panic.
    ///
    /// Refs: I-Core-NoPanic, I-Gov-NoCoreMutation
    fn verify_consistency(&self, session: &Session) -> PluginResult<Option<PolicyDecision>>;
}

/// Mandatory. Aggregates `before_prediction` decisions from multiple plugins.
///
/// The kernel refuses to start without an injected aggregator.
///
/// Refs: I-Gov-Decision-Required
/// # Complexity
/// O(d) where d = number of decisions. At least one linear pass.
/// # Panics
/// Never panics.
pub trait DecisionAggregator: Send + Sync {
    /// Reduce a vector of per-plugin decisions into a single decision.
    ///
    /// The kernel calls this after the `before_prediction` hook has been
    /// evaluated for every plugin. The returned decision determines
    /// whether prediction proceeds, is blocked, or is overridden.
    ///
    /// # Complexity
    /// O(d) where d = number of decisions.
    ///
    /// # Panics
    /// Never panics. Implementations must return `PluginError` rather
    /// than panic.
    ///
    /// Refs: I-Gov-Decision-Required
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
/// Refs: docs/SPECS.md §1.4, I-Shell-Drain-Atomic
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait SignalDrainOrder: Send + Sync {
    /// Drain pending signals from all channels into a batch.
    ///
    /// The returned `SignalDrainBatch` follows the canonical order:
    /// `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`.
    /// The kernel injects this batch into `ExtensionStorage` as
    /// `SignalBuffer` before calling `transition()`.
    ///
    /// # Complexity
    /// O(1). The trait call itself introduces no allocation.
    ///
    /// # Panics
    /// Never panics. Implementations must return an empty batch rather
    /// than panic if a channel is unavailable.
    ///
    /// Refs: I-Shell-Drain-Atomic
    fn drain(&self) -> crate::SignalDrainBatch;
}

/// Optional. O(1) validation of effects requested by plugins on specific hooks.
///
/// Without injection, all `RequestEffect`s are allowed on all hooks.
///
/// Refs: I-Core-HookEffect-O1
/// # Complexity
/// O(1). Bitwise AND on pre-computed masks.
/// # Panics
/// Never panics.
pub trait HookEffectConstraint: Send + Sync {
    /// O(1) validation by bitmask lookup.
    ///
    /// `hook_index`: compact hook index (0-7).
    /// `effect_mask`: bitmask of the effect to validate (`EffectBit` constant).
    ///
    /// # Complexity
    /// O(1). Bitwise AND on pre-computed masks.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-HookEffect-O1
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;

    /// Validation by name (fallback for custom/future extensions).
    ///
    /// Called when a hook or effect does not have a compact index.
    /// Implementations should match against known hook/effect names.
    ///
    /// # Complexity
    /// O(n) where n = number of known hook/effect pairs.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-HookEffect-O1
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
    ///
    /// `hook_name` identifies the hook for per-hook budget policies and
    /// telemetry. Implementations that do not need the name can ignore it.
    ///
    /// # Complexity
    /// O(1). Resets the internal frame state.
    ///
    /// # Panics
    /// Never panics.
    fn begin_hook(&mut self, hook_name: &'static str);

    /// Called by the kernel when an extension is mutated for the first time
    /// in this hook. The VTable `clone_box` provides the clone.
    ///
    /// # Complexity
    /// O(1) plus the cost of `clone_box` for the mutated extension.
    ///
    /// # Panics
    /// Never panics.
    fn on_mutation(&mut self, type_id: TypeId, vtable: &ExtVTable, current: &dyn Any);

    /// Called if the budget is respected — mutations are kept.
    ///
    /// # Complexity
    /// O(1). Clears the internal frame state.
    ///
    /// # Panics
    /// Never panics.
    fn commit_hook(&mut self, ext: &mut ExtensionStorage);

    /// Called if the budget is exceeded — restoration from snapshots.
    ///
    /// # Complexity
    /// O(k) where k = snapshotted extensions. Each is restored via
    /// `ExtensionStorage::restore_boxed`.
    ///
    /// # Panics
    /// Never panics.
    fn rollback_hook(&mut self, ext: &mut ExtensionStorage);

    /// Returns `true` if the current frame weight exceeded the configured budget.
    ///
    /// The kernel consults this after a monitored hook to decide whether to
    /// call `rollback_hook` or `commit_hook`. A default implementation that
    /// always returns `false` preserves the old no-rollback behavior for
    /// trivial implementations.
    ///
    /// # Complexity
    /// O(1). Scalar comparison.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    fn is_budget_exceeded(&self) -> bool {
        false
    }

    /// Attaches a per-hook COW budget policy.
    ///
    /// Implementations that do not support adaptive budgets can ignore the
    /// policy. The kernel calls this once during engine construction if a
    /// `CowBudgetPolicy` is configured via `BriocheEngineBuilder`.
    ///
    /// # Complexity
    /// O(1). Option assignment.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-CowBudget-Adaptative
    fn set_cow_budget_policy(&mut self, _policy: Box<dyn CowBudgetPolicy>) {}
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
    /// Clean up `SessionRegistry` when a sub-routine exits.
    ///
    /// Called on the outgoing transition from `AgentState::SubRoutine`.
    /// Implementations typically remove the child session from `registry`
    /// and may return effects (e.g. `ForwardToUi`) to notify the shell.
    ///
    /// # Complexity
    /// O(1) for the trait call. Implementations may incur lookup cost.
    ///
    /// # Panics
    /// Never panics. Implementations must return `PluginError` rather
    /// than panic.
    ///
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
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
/// Refs: docs/SPECS.md §2.10
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait GovernanceFailoverHandler: Send + Sync {
    /// Transform a `PluginFault` into a safe terminal effect sequence.
    ///
    /// Called when a governance trait or plugin returns a fatal error.
    /// Returning `Some(effects)` replaces the fault; returning `None`
    /// leaves the fault unchanged.
    ///
    /// # Complexity
    /// O(1). Pattern match on `Effect::PluginFault`.
    ///
    /// # Panics
    /// Never panics. Implementations must return `PluginError` rather
    /// than panic.
    ///
    /// Refs: I-Gov-Failover
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
/// Refs: docs/SPECS.md §2.11
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait CowBudgetPolicy: Send + Sync {
    /// Return the COW budget in bytes for the named hook.
    ///
    /// The `CycleRollbackPolicy` consults this value before each monitored
    /// hook. If cumulative snapshot weight exceeds this budget, the hook
    /// mutations are rolled back.
    ///
    /// # Complexity
    /// O(1). Implementations may use a lookup table.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Gov-CowBudget-Adaptative
    fn max_cow_bytes(&self, hook_name: &str) -> usize;
}
