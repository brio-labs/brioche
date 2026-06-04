use crate::{
    ConsistencyVerifier, CycleRollbackPolicy, DecisionAggregator, Effect, EpochInterceptor,
    GovernanceFailoverHandler, HookEffectConstraint, SessionRegistry, SubRoutineHandle,
    SubRoutineHandler, SubRoutineLifecycleGuard,
};

/// Governance trait container.
///
/// Holds all injectable policy traits and their orchestration state.
/// Separated from routing so that governance can evolve independently
/// of plugin dispatch mechanics.
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
/// Refs: I-Shell-Session-NoSend
pub struct RoutineManager {
    pub(crate) registry: SessionRegistry,
    pub(crate) next_generation_id: u64,
}

impl RoutineManager {
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
/// Refs: I-Core-PluginOrder, I-Gov-Decision-Required
pub(crate) enum InputResult {
    Allow,
    Block { reason: String },
    OverrideTransition(Vec<Effect>, &'static str),
    Accumulated(Vec<Effect>),
}
