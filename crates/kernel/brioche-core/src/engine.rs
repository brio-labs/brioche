//! Book I — The Core Book: `BriocheEngine` and `UnifiedRoutingTable`.
//!
//! The synchronous kernel. Never performs I/O. Computes transitions
//! from `(Session, EngineInput)` to `Vec<Effect>`.
//!
//! ## Invariants upheld
//! - I-Core-Pure: No side effects. Effects are declarative values.
//! - I-Core-NoPanic: Anomalies produce `Effect::Error` or `Effect::SystemIdle`.
//! - I-Core-StreamNoBranch: Pre-routed `UnifiedRoutingTable` provides O(1) dispatch.
//! - I-Core-PluginOrder: Total order via `(priority, name)`.
//! - I-Core-RetVecEffect: All outputs are `Vec<Effect>`.
//! - I-Core-ChunkBudget: Stream accumulation avoids allocation spikes.
//! - I-Core-ActiveToolCall: Tool descriptors are sealed before storage.
//! - I-Core-HookEffect-O1: Effect validation is O(1) via bitmask where possible.
//!
//! ## Entry points
//! - `BriocheEngine::transition()`: Main state transition function.
//! - `BriocheEngineBuilder`: Compile-time wiring of traits and plugins.
//!
//! Refs: docs/SPECS.md §4, §5; PHILOSOPHY.md §1, §2, §7

use crate::{
    ConsistencyVerifier, CycleRollbackPolicy, DecisionAggregator, Effect, EngineInput,
    EpochInterceptor, ErrorCode, ErrorDetail, GovernanceFailoverHandler, HookEffectConstraint,
    PluginSource, Session, SessionRegistry, SubRoutineHandle, SubRoutineHandler,
    SubRoutineLifecycleGuard,
};

mod builder;
mod dispatch;
mod finalize;
mod helpers;
mod hooks;
mod router;
mod trace;

pub use builder::{BriocheEngineBuilder, Missing, Present};
pub use router::{PluginRouter, UnifiedRoutingTable};

// ---------------------------------------------------------------------------
// Internal engine types (merged from engine/types.rs per PHILOSOPHY.md §3.3).
// ---------------------------------------------------------------------------

// Types use the same crate-level imports already present at the top of this file.

/// Governance trait container.
///
/// Holds all injectable policy traits and their orchestration state.
///
/// ## Architectural Note
/// Each optional trait is stored as `Box<dyn>`. This introduces one vtable
/// indirection per access. PHILOSOPHY.md §1 recommends pre-routing tables,
/// but governance traits have heterogeneous signatures that cannot be
/// flattened into a uniform table without erasing type safety.
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
/// # Complexity
/// O(1) for construction. Insert/remove: O(log n) where n = sub-routine count.
///
/// # Panics
/// Never panics.
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
            next_generation_id: 1,
        }
    }
}

/// Snapshot of sub-routine status taken at the start of `transition()`.
///
/// Used by lifecycle guards to detect transitions that exit a sub-routine.
///
/// # Complexity
/// O(1). Copied by value.
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
/// # Complexity
/// O(1) variant construction and match.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-PluginOrder, I-Gov-Decision-Required
pub(crate) enum InputResult {
    Allow,
    Block { detail: ErrorDetail },
    OverrideTransition(Vec<Effect>, PluginSource),
    Accumulated(Vec<Effect>),
}

// ---------------------------------------------------------------------------
// BriocheEngine
// ---------------------------------------------------------------------------

/// The synchronous kernel engine.
///
/// `BriocheEngine` is a thin facade over three independent components:
/// `PluginRouter` (dispatch), `GovernanceKernel` (policy traits), and
/// `RoutineManager` (sub-routine registry + generation counter).
///
/// Construct via `BriocheEngineBuilder`.
///
/// # Data Layout
/// Three owned fields: `PluginRouter` (~40 bytes + plugin vec),
/// `GovernanceKernel` (~136 bytes), `RoutineManager` (~48 bytes).
/// Total stack footprint ~224 bytes + heap-owned plugin/route state.
///
/// # Complexity
/// `transition()` is O(p + e) where p = active plugins, e = effects.
/// No allocation inside `transition()` beyond effects and rollback frames.
///
/// # Panics
/// Never panics. All anomalies produce `Effect::Error` or `Failure` state.
///
/// Refs: I-Core-Pure, I-Core-NoPanic
pub struct BriocheEngine {
    pub(crate) router: PluginRouter,
    pub(crate) governance: GovernanceKernel,
    pub(crate) routines: RoutineManager,
}

impl std::fmt::Debug for BriocheEngine {
    /// Debug representation — non-exhaustive to avoid leaking governance internals.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BriocheEngine")
            .field("plugin_count", &self.router.plugins.len())
            .field("next_generation_id", &self.routines.next_generation_id)
            .finish_non_exhaustive()
    }
}

impl BriocheEngine {
    /// Execute one synchronous transition cycle.
    ///
    /// The engine receives a `Session` and an `EngineInput`, computes the
    /// next state, and returns a vector of declarative `Effect`s for the
    /// shell to execute.
    ///
    /// This function never panics. Any anomaly produces an `Effect::Error`
    /// or transitions the automaton to a safe state.
    ///
    /// # Complexity
    /// O(p + e) where p = number of plugins on active routes, e = number of effects.
    ///
    /// Refs: I-Core-NoPanic, I-Core-RetVecEffect, I-Core-StreamNoBranch
    pub fn transition(&mut self, session: &mut Session, input: &EngineInput) -> Vec<Effect> {
        // Capture sub-routine state before any mutation.
        let pre = self.capture_pre_transition_state(session);

        // Inject a shared snapshot for all pre-mutation hooks.
        session.extensions.insert(session.snapshot());

        let mut effects = Vec::new();

        // EpochInterceptor (optional, but evaluated first if present).
        if self
            .apply_epoch_interceptor(session, input, &mut effects)
            .is_some()
        {
            self.finalize_transition(session, pre, &mut effects);
            return effects;
        }

        // SubRoutineHandler (optional, only when in sub-routine state).
        if self
            .apply_subroutine_handler(session, input, &mut effects)
            .is_some()
        {
            self.finalize_transition(session, pre, &mut effects);
            return effects;
        }

        // on_input hook (routed).
        match self.eval_on_input(session, input) {
            InputResult::OverrideTransition(ov_effects, source_plugin) => {
                self.log_override_transition(session, &source_plugin);
                effects.extend(ov_effects);
                self.finalize_transition(session, pre, &mut effects);
                return effects;
            }
            InputResult::Block { detail } => {
                effects.push(Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    detail,
                });
                effects.push(Effect::SystemIdle);
                self.finalize_transition(session, pre, &mut effects);
                return effects;
            }
            InputResult::Accumulated(acc) => {
                effects.extend(acc);
            }
            InputResult::Allow => {}
        }

        // Main dispatch on EngineInput variant.
        if let Err(err) = self.dispatch_input(session, input, &mut effects) {
            effects.push(Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::TransitionFailed {
                    reason: err.to_string(),
                },
            });
        }

        // Finalize.
        self.finalize_transition(session, pre, &mut effects);
        effects
    }

    /// Access the internal `SessionRegistry`.
    ///
    /// Refs: I-Shell-Session-NoSend
    /// Complexity: O(1). Returns a reference; no allocation.
    /// # Panics
    /// Never panics.
    pub fn session_registry(&self) -> &SessionRegistry {
        &self.routines.registry
    }

    /// Insert a sub-routine session into the registry.
    ///
    /// Refs: I-Shell-Session-NoSend
    /// Complexity: O(log n) where n = number of sub-routines.
    /// # Panics
    /// Never panics.
    pub fn create_subroutine(&mut self, handle: SubRoutineHandle, session: Session) {
        self.routines.registry.insert(handle, session);
    }

    /// Remove a sub-routine session from the registry.
    ///
    /// Refs: I-Shell-Session-NoSend
    /// Complexity: O(log n) where n = number of sub-routines.
    /// # Panics
    /// Never panics.
    pub fn remove_subroutine(&mut self, handle: &SubRoutineHandle) -> Option<Session> {
        self.routines.registry.remove(handle)
    }

    /// Rebuild routing tables excluding quarantined or inactive plugins.
    ///
    /// This is a transactional barrier: no new `EngineInput` should be
    /// processed until this call completes.
    ///
    /// # Complexity
    /// O(p log p) where p = number of plugins.
    ///
    /// # Panics
    /// Never panics. Invalid indices from `active_mask` are silently skipped.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    pub fn rebuild_routes(&mut self, active_mask: &[bool]) {
        let active_indices: Vec<usize> = (0..self.router.plugins.len())
            .filter(|i| match active_mask.get(*i) {
                Some(&b) => b,
                None => true,
            })
            .collect();

        self.router.routing_table =
            UnifiedRoutingTable::from_plugins_filtered(&self.router.plugins, &active_indices);
    }

    /// The default tool timeout applied when a descriptor omits `timeout_ms`.
    ///
    /// Refs: I-Core-ActiveToolCall
    /// Complexity: O(1). Scalar field access.
    /// # Panics
    /// Never panics.
    pub fn default_tool_timeout_ms(&self) -> u64 {
        self.governance.default_tool_timeout_ms
    }
}
