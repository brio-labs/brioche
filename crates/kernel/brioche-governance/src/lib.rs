#![deny(clippy::unwrap_used, clippy::expect_used)]
#![deny(missing_docs)]

//! # Brioche Governance — Book II
//!
//! Trait contracts and policy interfaces for the governance layer.
//! This crate defines the plugin trait system; implementations live in
//! `brioche-governance-default`.
//!
//! ## Public interface
//! - `BriochePlugin`: Core plugin metadata trait.
//! - `OnInput`, `BeforePrediction`, `OnStreamEvent`, `AfterPrediction`,
//!   `OnToolCalls`, `OnToolResult`, `OnError`: atomic capability traits.
//! - `PersistenceCap`: state persistence capability.
//! - `EpochInterceptor`, `SubRoutineHandler`, `DecisionAggregator`: governance
//!   anchor traits.
//! - `PluginCapabilities`: hook-subscription bitmask.
//!
//! ## Invariants upheld
//! - I-Gov-TraitAtomic: Each capability is a standalone trait.
//! - I-Gov-NoCoreMutation: Plugins never mutate `Session` directly.
//! - I-Gov-EffectExplicit: All policy decisions return typed effects.
//!
//! Refs: docs/SPECS.md §Book II

use std::any::Any;

/// Function pointer type for cloning a type-erased extension value.
///
/// Refs: I-Core-VTableClone
pub type CloneBoxFn = fn(&dyn Any) -> Box<dyn Any + Send + Sync>;

// ---------------------------------------------------------------------------
// BriocheTypes
// ---------------------------------------------------------------------------

/// Type family that connects governance traits to a concrete Book I kernel.
///
/// Implementations of this trait live in `brioche-core` (for example,
/// `CoreTypes`). It lets the governance crate remain agnostic of mechanism
/// types while preserving static, type-safe trait contracts.
///
/// Refs: I-Gov-TraitAtomic
pub trait BriocheTypes: 'static {
    /// Input delivered to `BriocheEngine::transition()`.
    type Input;
    /// Type-safe plugin-state container.
    type ExtensionStorage;
    /// Chat message in session history.
    type ChatMessage;
    /// LLM stream event.
    type StreamEvent;
    /// Decision returned by the streaming hook.
    type StreamAction;
    /// Descriptor for a tool call before sealing.
    type ToolCallDescriptor;
    /// Result DTO for a tool call.
    type ToolResultDTO;
    /// Declarative effect emitted by the kernel.
    type Effect;
    /// Plugin error taxonomy.
    type PluginError;
    /// Policy decision returned by gating hooks.
    type PolicyDecision;
    /// Epoch interception result.
    type EpochAction;
    /// Live session object.
    type Session;
    /// Live sub-routine registry.
    type SessionRegistry;
    /// Mechanical session state.
    type AgentState;
    /// System error taxonomy.
    type BriocheError;
    /// Batch of drained shell signals.
    type SignalDrainBatch;
    /// Governance notification payload.
    type GovernanceNotification;
    /// Async task result payload.
    type AsyncTaskResult;
    /// System signal payload.
    type SystemSignal;
    /// Structured source of a state inconsistency.
    type InconsistencySource;
    /// Plugin/source identifier.
    type PluginSource;
    /// Sub-routine handle.
    type SubRoutineHandle;
    /// Offloaded task identifier.
    type TaskId;
    /// Epoch state extension.
    type EpochState;
    /// Execution path tag.
    type ExecutionPath;
    /// Effect-bit constants container.
    type EffectBit;
}

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
    /// No hooks subscribed.
    pub const NONE: PluginCapabilities = PluginCapabilities(0);
    /// Subscribes to `on_input`.
    pub const ON_INPUT: PluginCapabilities = PluginCapabilities(1 << 0);
    /// Subscribes to `before_prediction`.
    pub const BEFORE_PREDICTION: PluginCapabilities = PluginCapabilities(1 << 1);
    /// Subscribes to `on_stream_event`.
    pub const ON_STREAM_EVENT: PluginCapabilities = PluginCapabilities(1 << 2);
    /// Subscribes to `after_prediction`.
    pub const AFTER_PREDICTION: PluginCapabilities = PluginCapabilities(1 << 3);
    /// Subscribes to `on_tool_calls`.
    pub const ON_TOOL_CALLS: PluginCapabilities = PluginCapabilities(1 << 4);
    /// Subscribes to `on_tool_result`.
    pub const ON_TOOL_RESULT: PluginCapabilities = PluginCapabilities(1 << 5);
    /// Subscribes to `on_error`.
    pub const ON_ERROR: PluginCapabilities = PluginCapabilities(1 << 6);

    /// Checks whether this set contains a capability.
    ///
    /// # Complexity
    /// O(1). Bitwise AND.
    /// # Panics
    /// Never panics.
    pub const fn contains(self, other: PluginCapabilities) -> bool {
        (self.0 & other.0) == other.0
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
        PluginCapabilities(self.0 | rhs.0)
    }
}

// ---------------------------------------------------------------------------
// Atomic capability traits
// ---------------------------------------------------------------------------

/// Input interceptor capability.
///
/// Allows a governance plugin to replace or gate the standard dispatch.
///
/// Refs: I-Core-NoPanic, I-Gov-TraitAtomic
pub trait OnInput<T: BriocheTypes>: Send + Sync
where
    T::PolicyDecision: Default,
{
    /// Intercept or transform an incoming input.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    /// # Panics
    /// Never panics in the default implementation.
    fn on_input(
        &self,
        _input: &T::Input,
        _ext: &mut T::ExtensionStorage,
    ) -> Result<T::PolicyDecision, T::PluginError> {
        let _ = (_input, _ext);
        Ok(T::PolicyDecision::default())
    }
}

/// Pre-prediction gating capability.
///
    /// Decisions are collected and passed to `DecisionAggregator`.
///
/// Refs: I-Core-NoPanic, I-Gov-Decision-Required, I-Gov-TraitAtomic
pub trait BeforePrediction<T: BriocheTypes>: Send + Sync
where
    T::PolicyDecision: Default,
{
    /// Inspect history and return a policy decision.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    /// # Panics
    /// Never panics in the default implementation.
    fn before_prediction(
        &self,
        _history: &[T::ChatMessage],
        _ext: &mut T::ExtensionStorage,
    ) -> Result<T::PolicyDecision, T::PluginError> {
        let _ = (_history, _ext);
        Ok(T::PolicyDecision::default())
    }
}

/// Stream event handling capability — the hot path.
///
/// Plugins return `StreamAction` to avoid branching in the streaming loop.
///
/// Refs: I-Core-StreamNoBranch, I-Gov-TraitAtomic
pub trait OnStreamEvent<T: BriocheTypes>: Send + Sync
where
    T::StreamAction: Default,
{
    /// React to a single stream event.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    /// # Panics
    /// Never panics in the default implementation.
    fn on_stream_event(
        &self,
        _event: &T::StreamEvent,
        _ext: &mut T::ExtensionStorage,
    ) -> Result<T::StreamAction, T::PluginError> {
        let _ = (_event, _ext);
        Ok(T::StreamAction::default())
    }
}

/// Post-prediction capability.
///
/// Called after prediction completes, before tool execution or idle.
///
/// Refs: I-Core-NoPanic, I-Gov-TraitAtomic
pub trait AfterPrediction<T: BriocheTypes>: Send + Sync {
    /// React to completion of the prediction phase.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    /// # Panics
    /// Never panics in the default implementation.
    fn after_prediction(
        &self,
        _ext: &mut T::ExtensionStorage,
    ) -> Result<(), T::PluginError> {
        let _ = _ext;
        Ok(())
    }
}

/// Tool-call mutation capability.
///
/// Called before emission of the `ExecuteTools` effect. Plugins mutate
/// descriptors in place.
///
/// Refs: I-Core-ActiveToolCall, I-Gov-TraitAtomic
pub trait OnToolCalls<T: BriocheTypes>: Send + Sync {
    /// Inspect or mutate pending tool-call descriptors.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    /// # Panics
    /// Never panics in the default implementation.
    fn on_tool_calls(
        &self,
        _calls: &mut Vec<T::ToolCallDescriptor>,
        _ext: &mut T::ExtensionStorage,
    ) -> Result<(), T::PluginError> {
        let _ = (_calls, _ext);
        Ok(())
    }
}

/// Tool-result mutation capability.
///
/// Called before persistence of tool results in history. Plugins mutate
/// results in place.
///
/// Refs: I-Core-ActiveToolCall, I-Gov-TraitAtomic
pub trait OnToolResult<T: BriocheTypes>: Send + Sync {
    /// Inspect or mutate tool results.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    /// # Panics
    /// Never panics in the default implementation.
    fn on_tool_result(
        &self,
        _results: &mut Vec<T::ToolResultDTO>,
        _ext: &mut T::ExtensionStorage,
    ) -> Result<(), T::PluginError> {
        let _ = (_results, _ext);
        Ok(())
    }
}

/// Error-reactive capability.
///
/// Called when a plugin error is intercepted.
///
/// Refs: I-Core-NoPanic, I-Gov-TraitAtomic
pub trait OnError<T: BriocheTypes>: Send + Sync
where
    T::PolicyDecision: Default,
{
    /// React to an intercepted plugin fault.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Plugin-defined otherwise.
    /// # Panics
    /// Never panics in the default implementation.
    fn on_error(
        &self,
        _error: &T::PluginError,
        _ext: &mut T::ExtensionStorage,
    ) -> Result<T::PolicyDecision, T::PluginError> {
        let _ = (_error, _ext);
        Ok(T::PolicyDecision::default())
    }
}

// ---------------------------------------------------------------------------
// Persistence capability
// ---------------------------------------------------------------------------

/// Persistence capability for plugins that store extension state.
///
/// Separated from hook capabilities so that stateless plugins need not
/// carry empty default methods.
///
/// Refs: I-Persist-Idempotence, I-Gov-TraitAtomic
pub trait PersistenceCap: Send + Sync {
    /// Reserved keys in extension storage. Format: `"plugin_name::state_name"`.
    ///
    /// # Complexity
    /// O(1). Returns a static slice.
    /// # Panics
    /// Never panics.
    fn owned_state_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Serializes the default state into a binary blob.
    ///
    /// # Complexity
    /// O(1) for the default implementation.
    /// # Panics
    /// Never panics in the default implementation.
    fn default_state_blob(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Attempts to deserialize a blob. On failure, the engine calls
    /// `default_state_blob`.
    ///
    /// # Complexity
    /// O(1) for the default implementation.
    /// # Panics
    /// Never panics in the default implementation.
    fn deserialize_state(
        &self,
        _raw: &[u8],
    ) -> Result<Box<dyn Any + Send + Sync>, String> {
        Err("Not implemented".into())
    }
}

// ---------------------------------------------------------------------------
// BriochePlugin
// ---------------------------------------------------------------------------

/// Core plugin metadata trait.
///
/// A plugin implements `BriochePlugin<T>` for the shared type family `T`
/// (e.g. `brioche_core::CoreTypes`) and the atomic capability traits that
/// correspond to its `capabilities()` mask. The engine uses the mask to
/// pre-compute routes and the `as_*` accessors to obtain the trait object
/// for a given capability at dispatch time.
///
/// Refs: I-Core-PluginOrder, I-Gov-NoCoreMutation, I-Gov-TraitAtomic
/// # Panics
/// Panics only if an index is out of bounds; callers must validate lengths.
/// Core plugin metadata and hook-dispatch trait.
///
/// A plugin implements `BriochePlugin<T>` for the shared type family `T`
/// (e.g. `brioche_core::CoreTypes`). Hook implementations can be provided
/// either by overriding the methods on this trait directly, or — preferred —
/// by implementing the corresponding atomic capability trait (`OnInput<T>`,
/// `BeforePrediction<T>`, ...) and letting the default `BriochePlugin`
/// methods delegate via the `as_*` accessors. The `#[brioche_plugin]` macro
/// generates the accessors from the `capabilities` mask.
///
/// Refs: I-Core-PluginOrder, I-Gov-NoCoreMutation, I-Gov-TraitAtomic
/// # Panics
/// Panics only if an index is out of bounds; callers must validate lengths.
pub trait BriochePlugin<T: BriocheTypes>: Send + Sync + PersistenceCap
where
    T::PolicyDecision: Default,
    T::StreamAction: Default,
{
    /// Unique plugin name, used for total ordering and traceability.
    ///
    /// # Complexity
    /// O(1). Returns a static string.
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-PluginOrder
    fn name(&self) -> &'static str;

    /// Hook subscriptions. Determines which routes the plugin is placed on.
    ///
    /// # Complexity
    /// O(1). Returns a bitmask.
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-StreamNoBranch
    fn capabilities(&self) -> PluginCapabilities;

    /// Deterministic evaluation order. Lower priority = evaluated first.
    ///
    /// # Complexity
    /// O(1). Returns a scalar.
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Core-PluginOrder
    fn priority(&self) -> i16 {
        0
    }

    /// Input interceptor hook.
    ///
    /// Default delegates to `OnInput<T>` if the plugin exposes it.
    ///
    /// Refs: I-Core-NoPanic
    fn on_input(
        &self,
        input: &T::Input,
        ext: &mut T::ExtensionStorage,
    ) -> Result<T::PolicyDecision, T::PluginError> {
        match self.as_on_input() {
            Some(cap) => cap.on_input(input, ext),
            None => Ok(T::PolicyDecision::default()),
        }
    }

    /// Pre-prediction gating hook.
    ///
    /// Default delegates to `BeforePrediction<T>` if the plugin exposes it.
    ///
    /// Refs: I-Core-NoPanic, I-Gov-Decision-Required
    fn before_prediction(
        &self,
        history: &[T::ChatMessage],
        ext: &mut T::ExtensionStorage,
    ) -> Result<T::PolicyDecision, T::PluginError> {
        match self.as_before_prediction() {
            Some(cap) => cap.before_prediction(history, ext),
            None => Ok(T::PolicyDecision::default()),
        }
    }

    /// Stream event hook.
    ///
    /// Default delegates to `OnStreamEvent<T>` if the plugin exposes it.
    ///
    /// Refs: I-Core-StreamNoBranch
    fn on_stream_event(
        &self,
        event: &T::StreamEvent,
        ext: &mut T::ExtensionStorage,
    ) -> Result<T::StreamAction, T::PluginError> {
        match self.as_on_stream_event() {
            Some(cap) => cap.on_stream_event(event, ext),
            None => Ok(T::StreamAction::default()),
        }
    }

    /// Post-prediction hook.
    ///
    /// Default delegates to `AfterPrediction<T>` if the plugin exposes it.
    ///
    /// Refs: I-Core-NoPanic
    fn after_prediction(&self, ext: &mut T::ExtensionStorage) -> Result<(), T::PluginError> {
        match self.as_after_prediction() {
            Some(cap) => cap.after_prediction(ext),
            None => Ok(()),
        }
    }

    /// Tool-call mutation hook.
    ///
    /// Default delegates to `OnToolCalls<T>` if the plugin exposes it.
    ///
    /// Refs: I-Core-ActiveToolCall
    fn on_tool_calls(
        &self,
        calls: &mut Vec<T::ToolCallDescriptor>,
        ext: &mut T::ExtensionStorage,
    ) -> Result<(), T::PluginError> {
        match self.as_on_tool_calls() {
            Some(cap) => cap.on_tool_calls(calls, ext),
            None => Ok(()),
        }
    }

    /// Tool-result mutation hook.
    ///
    /// Default delegates to `OnToolResult<T>` if the plugin exposes it.
    ///
    /// Refs: I-Core-ActiveToolCall
    fn on_tool_result(
        &self,
        results: &mut Vec<T::ToolResultDTO>,
        ext: &mut T::ExtensionStorage,
    ) -> Result<(), T::PluginError> {
        match self.as_on_tool_result() {
            Some(cap) => cap.on_tool_result(results, ext),
            None => Ok(()),
        }
    }

    /// Error-reactive hook.
    ///
    /// Default delegates to `OnError<T>` if the plugin exposes it.
    ///
    /// Refs: I-Core-NoPanic
    fn on_error(
        &self,
        error: &T::PluginError,
        ext: &mut T::ExtensionStorage,
    ) -> Result<T::PolicyDecision, T::PluginError> {
        match self.as_on_error() {
            Some(cap) => cap.on_error(error, ext),
            None => Ok(T::PolicyDecision::default()),
        }
    }

    /// Access the `OnInput` capability, if implemented.
    ///
    /// The default returns `None`. Capability-specific plugins override this
    /// via the `#[brioche_plugin]` macro.
    ///
    /// # Complexity
    /// O(1).
    /// # Panics
    /// Never panics.
    fn as_on_input(&self) -> Option<&dyn OnInput<T>> {
        None
    }

    /// Access the `BeforePrediction` capability, if implemented.
    ///
    /// # Complexity
    /// O(1).
    /// # Panics
    /// Never panics.
    fn as_before_prediction(&self) -> Option<&dyn BeforePrediction<T>> {
        None
    }

    /// Access the `OnStreamEvent` capability, if implemented.
    ///
    /// # Complexity
    /// O(1).
    /// # Panics
    /// Never panics.
    fn as_on_stream_event(&self) -> Option<&dyn OnStreamEvent<T>> {
        None
    }

    /// Access the `AfterPrediction` capability, if implemented.
    ///
    /// # Complexity
    /// O(1).
    /// # Panics
    /// Never panics.
    fn as_after_prediction(&self) -> Option<&dyn AfterPrediction<T>> {
        None
    }

    /// Access the `OnToolCalls` capability, if implemented.
    ///
    /// # Complexity
    /// O(1).
    /// # Panics
    /// Never panics.
    fn as_on_tool_calls(&self) -> Option<&dyn OnToolCalls<T>> {
        None
    }

    /// Access the `OnToolResult` capability, if implemented.
    ///
    /// # Complexity
    /// O(1).
    /// # Panics
    /// Never panics.
    fn as_on_tool_result(&self) -> Option<&dyn OnToolResult<T>> {
        None
    }

    /// Access the `OnError` capability, if implemented.
    ///
    /// # Complexity
    /// O(1).
    /// # Panics
    /// Never panics.
    fn as_on_error(&self) -> Option<&dyn OnError<T>> {
        None
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
pub trait EpochInterceptor<T: BriocheTypes>: Send + Sync {
    /// Compare the input's epoch with the current epoch.
    ///
    /// Refs: I-Comp-Epoch-First, I-Gov-Epoch-Reject
    fn intercept_epoch(
        &self,
        input: &T::Input,
        ext: &mut T::ExtensionStorage,
    ) -> Result<T::EpochAction, T::PluginError>;
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
pub trait SubRoutineHandler<T: BriocheTypes>: Send + Sync {
    /// Delegate the input to the child session or bubble a terminal result.
    ///
    /// Refs: I-Comp-Epoch-Subroutine
    fn handle_subroutine(
        &self,
        session: &mut T::Session,
        registry: &mut T::SessionRegistry,
        input: &T::Input,
    ) -> Result<Option<Vec<T::Effect>>, T::BriocheError>;
}

/// Hydrates a sub-routine session from a persisted binary head blob.
///
/// This is a boundary trait: `brioche-core` defines the concrete types via
/// `BriocheTypes`, while `brioche-shell-persistence` supplies the
/// MessagePack implementation.
///
/// # Complexity
/// O(deserialization cost). The trait itself introduces no allocation.
/// # Panics
/// Never panics. Implementations must return `BriocheError` on failure.
///
/// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
pub trait SubRoutineHydrator<T: BriocheTypes>: Send + Sync {
    /// Deserialize a binary blob into a live `Session`.
    ///
    /// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
    fn hydrate(&self, raw: &[u8]) -> Result<T::Session, T::BriocheError>;
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
pub trait ConsistencyVerifier<T: BriocheTypes>: Send + Sync {
    /// Verify that the session is in a mechanically consistent state.
    ///
    /// Refs: I-Core-NoPanic, I-Gov-NoCoreMutation
    fn verify_consistency(
        &self,
        session: &T::Session,
    ) -> Result<Option<Vec<T::Effect>>, T::PluginError>;
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
pub trait DecisionAggregator<T: BriocheTypes>: Send + Sync {
    /// Aggregate a sequence of plugin decisions into a single decision.
    ///
    /// Refs: I-Gov-Decision-Required
    fn aggregate_decisions(
        &self,
        decisions: &[(T::PluginSource, T::PolicyDecision)],
    ) -> Result<T::PolicyDecision, T::PluginError>;
}

/// Defines the invariant drainage order of separate channels.
///
/// The shell implements this trait and the engine thread loop calls
/// `drain()` before each `transition()` cycle. The returned batch is
/// injected into `ExtensionStorage` as `SignalBuffer` so that plugins
/// can consume pending signals in their hooks.
///
/// Canonical order: `SystemSignal` -> `GovernanceNotification` -> `AsyncTaskResult`.
///
/// Refs: docs/SPECS.md §1.4, I-Shell-Drain-Atomic
/// # Complexity
/// O(1). No heap allocation.
/// # Panics
/// Never panics.
pub trait SignalDrainOrder<T: BriocheTypes>: Send + Sync {
    /// Drain pending signals from the shell channels.
    ///
    /// Refs: docs/SPECS.md §1.4, I-Shell-Drain-Atomic
    fn drain(&self) -> T::SignalDrainBatch;
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
pub trait HookEffectConstraint<T: BriocheTypes>: Send + Sync {
    /// Validate an effect bitmask for a hook index.
    ///
    /// Refs: I-Core-HookEffect-O1
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;

    /// Validate a concrete effect for a hook index.
    ///
    /// Refs: I-Core-HookEffect-O1
    fn is_allowed(&self, hook_index: u8, effect: &T::Effect) -> bool;
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
pub trait CycleRollbackPolicy<T: BriocheTypes>: Send + Sync {
    /// Called before each monitored hook.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    fn begin_hook(
        &mut self, hook_name: &'static str);

    /// Called on the first mutation of an extension in the current hook.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    fn on_mutation(
        &mut self,
        ext: &mut T::ExtensionStorage,
        key: &str,
        clone_box: CloneBoxFn,
    );

    /// Called at the end of a hook if the budget is respected.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    fn commit_hook(
        &mut self, ext: &mut T::ExtensionStorage);

    /// Called at the end of a hook if the budget is exceeded.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    fn rollback_hook(
        &mut self, ext: &mut T::ExtensionStorage);
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
pub trait SubRoutineLifecycleGuard<T: BriocheTypes>: Send + Sync {
    /// Clean up the registry when the automaton leaves the `SubRoutine` state.
    ///
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
    fn on_exit(
        &self,
        session: &T::Session,
        registry: &mut T::SessionRegistry,
    ) -> Result<Option<Vec<T::Effect>>, T::PluginError>;
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
pub trait GovernanceFailoverHandler<T: BriocheTypes>: Send + Sync {
    /// Transform a `PluginFault` effect into a safe terminal effect sequence.
    ///
    /// Refs: docs/SPECS.md §2.10, I-Gov-Failover
    fn handle_failure(
        &self,
        session: &T::Session,
        fault: &T::Effect,
    ) -> Result<Option<Vec<T::Effect>>, T::PluginError>;
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
    /// Maximum COW snapshot bytes allowed for the named hook.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    fn max_cow_bytes(
        &self, hook_name: &'static str) -> usize;
}
