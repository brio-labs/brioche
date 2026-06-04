//! Type-state builder for `BriocheEngine`.
//!
//! Refs: SPECS

use super::{PluginRouter, UnifiedRoutingTable};
use crate::{
    BriocheEngine, BriochePlugin, ConsistencyVerifier, CycleRollbackPolicy, DecisionAggregator,
    EpochInterceptor, GovernanceFailoverHandler, HookEffectConstraint, SubRoutineHandler,
    SubRoutineLifecycleGuard,
};

/// Type-state marker: mandatory trait not yet injected.
///
/// Refs: SPECS
pub struct Missing;

/// Type-state marker: mandatory trait injected.
///
/// Refs: SPECS
pub struct Present;

/// Builder for `BriocheEngine`.
///
/// Uses type-state to enforce injection of mandatory governance traits
/// (`DecisionAggregator`, `SubRoutineLifecycleGuard`) at **compile time**.
/// Calling `build()` before both traits are injected is a compile error.
///
/// Refs: SPECS
pub struct BriocheEngineBuilder<DA = Missing, LG = Missing> {
    plugins: Vec<Box<dyn BriochePlugin>>,
    epoch_interceptor: Option<Box<dyn EpochInterceptor>>,
    subroutine_handler: Option<Box<dyn SubRoutineHandler>>,
    consistency_verifier: Option<Box<dyn ConsistencyVerifier>>,
    decision_aggregator: Option<Box<dyn DecisionAggregator>>,
    hook_effect_constraint: Option<Box<dyn HookEffectConstraint>>,
    cycle_rollback_policy: Option<Box<dyn CycleRollbackPolicy>>,
    subroutine_lifecycle_guard: Option<Box<dyn SubRoutineLifecycleGuard>>,
    governance_failover_handler: Option<Box<dyn GovernanceFailoverHandler>>,
    default_tool_timeout_ms: u64,
    _phantom: std::marker::PhantomData<(DA, LG)>,
}

impl Default for BriocheEngineBuilder<Missing, Missing> {
    /// Default builder in the `Missing, Missing` state.
    ///
    /// Refs: SPECS
    fn default() -> Self {
        Self::new()
    }
}

impl BriocheEngineBuilder<Missing, Missing> {
    /// Create a new builder with no plugins or governance traits.
    ///
    /// Refs: SPECS
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            epoch_interceptor: None,
            subroutine_handler: None,
            consistency_verifier: None,
            decision_aggregator: None,
            hook_effect_constraint: None,
            cycle_rollback_policy: None,
            subroutine_lifecycle_guard: None,
            governance_failover_handler: None,
            default_tool_timeout_ms: 0,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<DA, LG> BriocheEngineBuilder<DA, LG> {
    fn change_type<NewDA, NewLG>(self) -> BriocheEngineBuilder<NewDA, NewLG> {
        BriocheEngineBuilder {
            plugins: self.plugins,
            epoch_interceptor: self.epoch_interceptor,
            subroutine_handler: self.subroutine_handler,
            consistency_verifier: self.consistency_verifier,
            decision_aggregator: self.decision_aggregator,
            hook_effect_constraint: self.hook_effect_constraint,
            cycle_rollback_policy: self.cycle_rollback_policy,
            subroutine_lifecycle_guard: self.subroutine_lifecycle_guard,
            governance_failover_handler: self.governance_failover_handler,
            default_tool_timeout_ms: self.default_tool_timeout_ms,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Register a plugin.
    ///
    /// Plugins are sorted by `(priority, name)` at `build()` time.
    ///
    /// Refs: I-Core-PluginOrder
    pub fn with_plugin(mut self, plugin: Box<dyn BriochePlugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Inject an `EpochInterceptor`.
    ///
    /// Refs: I-Comp-Epoch-First
    pub fn with_epoch_interceptor(mut self, interceptor: Box<dyn EpochInterceptor>) -> Self {
        self.epoch_interceptor = Some(interceptor);
        self
    }

    /// Inject a `SubRoutineHandler`.
    ///
    /// Refs: I-Comp-Epoch-Subroutine
    pub fn with_subroutine_handler(mut self, handler: Box<dyn SubRoutineHandler>) -> Self {
        self.subroutine_handler = Some(handler);
        self
    }

    /// Inject a `ConsistencyVerifier`.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    pub fn with_consistency_verifier(mut self, verifier: Box<dyn ConsistencyVerifier>) -> Self {
        self.consistency_verifier = Some(verifier);
        self
    }

    /// Inject a `HookEffectConstraint`.
    ///
    /// Refs: I-Core-HookEffect-O1
    pub fn with_hook_effect_constraint(
        mut self,
        constraint: Box<dyn HookEffectConstraint>,
    ) -> Self {
        self.hook_effect_constraint = Some(constraint);
        self
    }

    /// Inject a `CycleRollbackPolicy`.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    pub fn with_cycle_rollback_policy(mut self, policy: Box<dyn CycleRollbackPolicy>) -> Self {
        self.cycle_rollback_policy = Some(policy);
        self
    }

    /// Inject a `GovernanceFailoverHandler`.
    ///
    /// Refs: I-Gov-Failover
    pub fn with_governance_failover_handler(
        mut self,
        handler: Box<dyn GovernanceFailoverHandler>,
    ) -> Self {
        self.governance_failover_handler = Some(handler);
        self
    }

    /// Set the default tool timeout applied when a descriptor omits
    /// `timeout_ms`.
    ///
    /// This is a mechanical safeguard, not a policy decision. The kernel
    /// applies this value during `seal()` when no plugin has set a timeout.
    ///
    /// Refs: I-Core-ActiveToolCall
    pub fn with_default_tool_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.default_tool_timeout_ms = timeout_ms;
        self
    }
}

impl<LG> BriocheEngineBuilder<Missing, LG> {
    /// Inject a `DecisionAggregator` (mandatory).
    ///
    /// Transitions the builder type from `Missing` to `Present`.
    ///
    /// Refs: I-Gov-Decision-Required
    pub fn with_decision_aggregator(
        mut self,
        aggregator: Box<dyn DecisionAggregator>,
    ) -> BriocheEngineBuilder<Present, LG> {
        self.decision_aggregator = Some(aggregator);
        self.change_type()
    }
}

impl<LG> BriocheEngineBuilder<Present, LG> {
    /// Override the `DecisionAggregator`.
    ///
    /// Refs: I-Gov-Decision-Required
    pub fn with_decision_aggregator(mut self, aggregator: Box<dyn DecisionAggregator>) -> Self {
        self.decision_aggregator = Some(aggregator);
        self
    }
}

impl<DA> BriocheEngineBuilder<DA, Missing> {
    /// Inject a `SubRoutineLifecycleGuard` (mandatory).
    ///
    /// Transitions the builder type from `Missing` to `Present`.
    ///
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
    pub fn with_subroutine_lifecycle_guard(
        mut self,
        guard: Box<dyn SubRoutineLifecycleGuard>,
    ) -> BriocheEngineBuilder<DA, Present> {
        self.subroutine_lifecycle_guard = Some(guard);
        self.change_type()
    }
}

impl<DA> BriocheEngineBuilder<DA, Present> {
    /// Override the `SubRoutineLifecycleGuard`.
    ///
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
    pub fn with_subroutine_lifecycle_guard(
        mut self,
        guard: Box<dyn SubRoutineLifecycleGuard>,
    ) -> Self {
        self.subroutine_lifecycle_guard = Some(guard);
        self
    }
}

impl BriocheEngineBuilder<Present, Present> {
    /// Build the `BriocheEngine`.
    ///
    /// Both mandatory traits are guaranteed present by the type system.
    /// This method never fails.
    ///
    /// Refs: SPECS
    pub fn build(self) -> BriocheEngine {
        let routing_table = UnifiedRoutingTable::from_plugins(&self.plugins);

        BriocheEngine {
            router: PluginRouter {
                plugins: self.plugins,
                routing_table,
            },
            governance: crate::engine::GovernanceKernel {
                epoch_interceptor: self.epoch_interceptor,
                subroutine_handler: self.subroutine_handler,
                consistency_verifier: self.consistency_verifier,
                decision_aggregator: self.decision_aggregator,
                hook_effect_constraint: self.hook_effect_constraint,
                cycle_rollback_policy: self.cycle_rollback_policy,
                subroutine_lifecycle_guard: self.subroutine_lifecycle_guard,
                governance_failover_handler: self.governance_failover_handler,
                default_tool_timeout_ms: self.default_tool_timeout_ms,
            },
            routines: crate::engine::RoutineManager::new(),
        }
    }
}
