//! Book I — The Core Book: Internal engine types.
//!
//! Governance kernel container, routine manager, and transition state snapshots.
//! The structs in this module are `pub` (their fields are `pub(crate)`) so
//! that `BriocheEngine` can own them; they are not re-exported at the crate
//! root and are therefore not part of the public API.
//!
//! ## Invariants upheld
//! - I-Core-Pure: No side effects in these types.
//! - I-Shell-Session-NoSend: `RoutineManager` owns `!Send + !Sync` sessions.
//!
//! Refs: SPECS.md §4

use crate::{
    ConsistencyVerifier, CycleRollbackPolicy, DecisionAggregator, Effect, EpochInterceptor,
    ErrorDetail, GovernanceFailoverHandler, HookEffectConstraint, PluginSource, SessionRegistry,
    SubRoutineHandle, SubRoutineHandler, SubRoutineLifecycleGuard,
};

/// Governance trait container.
///
/// Holds all injectable policy traits and their orchestration state.
/// Separated from routing so that governance can evolve independently
/// of plugin dispatch mechanics.
///
/// ## Architectural Note
/// Each optional trait is stored as `Box<dyn>`. This introduces one vtable
/// indirection per access. PHILOSOPHY.md §1 recommends pre-routing tables,
/// but governance traits have heterogeneous signatures that cannot be
/// flattened into a uniform table without erasing type safety. This is a
/// known architectural debt tracked under the governance-trait-dispatch
/// improvement theme.
///
/// # Data Layout
/// Nine fields: eight `Option<Box<dyn Trait>>` pointers (16 bytes each on
/// x86_64: data pointer + vtable pointer) plus one `u64`. Total: ~136 bytes.
/// All heap allocation happens at engine-build time; `transition()` only
/// dereferences existing boxes.
///
/// # Complexity
/// O(1) field access per governance phase. No allocation in `transition()`.
///
/// # Panics
/// Never panics. All fields are `Option`; absent traits are silently skipped.
///
/// Refs: I-Comp-Epoch-First, I-Gov-Decision-Required
pub struct GovernanceKernel {
    pub(crate) epoch_interceptor: Option<Box<dyn EpochInterceptor>>,
    pub(crate) subroutine_handler: Option<Box<dyn SubRoutineHandler>>,
    pub(crate) consistency_verifier: Option<Box<dyn ConsistencyVerifier>>,
    pub(crate) decision_aggregator: Option<Box<dyn DecisionAggregator>>,
    pub(crate) hook_effect_constraint: Option<Box<dyn HookEffectConstraint>>,
    pub(crate) cycle_rollback_policy: Option<Box<dyn CycleRollbackPolicy>>,
    pub(crate) subroutine_lifecycle_guard: Option<Box<dyn SubRoutineLifecycleGuard>>,
    pub(crate) governance_failover_handler: Option<Box<dyn GovernanceFailoverHandler>>,
    pub(crate) default_tool_timeout_ms: u64,
}

/// Sub-routine session registry and generation counter.
///
/// Owns live `Session` instances for sub-routines and tracks the
/// monotonically increasing prediction generation ID.
///
/// # Data Layout
/// `SessionRegistry` (typically a `BTreeMap<SubRoutineHandle, Session>`) plus
/// one `u64`. Estimated footprint: ~48 bytes + map entries.
///
/// # Complexity
/// `new()`: O(1). `registry` insert/remove: O(log n) where n = sub-routine count.
///
/// # Panics
/// Never panics. `next_generation_id` overflows are accepted as wrap-around
/// (u64 range is effectively inexhaustible for the engine lifetime).
///
/// Refs: I-Shell-Session-NoSend
pub struct RoutineManager {
    pub(crate) registry: SessionRegistry,
    pub(crate) next_generation_id: u64,
}

impl RoutineManager {
    /// Create a new `RoutineManager` with an empty registry.
    ///
    /// # Complexity
    /// O(1). Allocates empty collections.
    ///
    /// # Panics
    /// Never panics.
    ///
    /// Refs: I-Shell-Session-NoSend
    pub(crate) fn new() -> Self {
        Self {
            registry: SessionRegistry::new(),
            // Reserve 0 so that generation_id == 0 always signals "none / invalid".
            next_generation_id: 1,
        }
    }
}

/// Snapshot of sub-routine status taken at the start of `transition()`.
///
/// Used by lifecycle guards to detect transitions that exit a sub-routine.
///
/// # Data Layout
/// Two fields: one `bool` + one `Option<SubRoutineHandle>` (typically a
/// small string or handle struct). Total: ~32 bytes. Stack-only; no heap.
///
/// # Complexity
/// O(1). Copied by value into `finalize_transition`.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Gov-SubRoutineLifecycle-Guard
pub(crate) struct PreTransitionState {
    pub(crate) was_subroutine: bool,
    pub(crate) handle: Option<SubRoutineHandle>,
}

/// Result of evaluating the `on_input` hook route.
///
/// Encodes the three terminal actions a plugin can take on input, plus
/// the accumulation of non-terminal effects.
///
/// # Data Layout
/// Four variants. `Block` carries `ErrorDetail` (structured, ≤ ~64 bytes).
/// `OverrideTransition` carries a `Vec<Effect>` (heap) + `PluginSource` (copy).
/// `Accumulated` carries a `Vec<Effect>` (heap). `Allow` is ZST.
///
/// # Complexity
/// O(1) variant construction and match. `Vec` moves are pointer copies.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-PluginOrder, I-Gov-Decision-Required
pub(crate) enum InputResult {
    /// Input is allowed to proceed to dispatch.
    Allow,
    /// Input is blocked; emit the structured error detail and idle.
    Block { detail: ErrorDetail },
    /// A plugin forced a custom transition; effects are emitted and the
    /// normal dispatch phase is skipped.
    OverrideTransition(Vec<Effect>, PluginSource),
    /// Non-terminal effects accumulated from `RequestEffect` decisions.
    Accumulated(Vec<Effect>),
}
