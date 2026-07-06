//! Type-state builder for `BriocheEngine`.
//!
//! Refs: docs/SPECS.md §4; PHILOSOPHY.md §1, §2

use super::{PluginRouter, UnifiedRoutingTable};
use crate::{
    AfterPredictionPlugin, BeforePredictionPlugin, BriocheEngine, ConsistencyVerifierPlugin,
    CowBudgetPolicyPlugin, CycleRollbackPolicyPlugin, DecisionAggregatorPlugin,
    EpochInterceptorPlugin, GovernanceFailoverHandlerPlugin, HookEffectConstraint, OnErrorPlugin,
    OnInputPlugin, OnStreamEventPlugin, OnToolCallsPlugin, OnToolResultPlugin,
    SubRoutineHandlerPlugin, SubRoutineHydratorPlugin, SubRoutineLifecycleGuardPlugin,
};

/// Type-state marker: mandatory trait not yet injected.
///
/// # Complexity
/// O(1). Zero-sized type.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-BuilderTypeState
pub struct Missing;

/// Type-state marker: mandatory trait injected.
///
/// # Complexity
/// O(1). Zero-sized type.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-BuilderTypeState
pub struct Present;

/// Builder for `BriocheEngine`.
///
/// Uses type-state to enforce injection of mandatory governance traits
/// (`DecisionAggregator`, `SubRoutineLifecycleGuard`) at compile time.
/// Hook registration is capability-specific, so each registered plugin has
/// exactly one lifecycle capability.
///
/// # Complexity
/// O(1) per setter. O(p log p) at `build()` time where p = hook count.
///
/// # Panics
/// Never panics.
///
/// Refs: I-Core-BuilderTypeState, I-Gov-Decision-Required, I-Gov-TraitAtomic
pub struct BriocheEngineBuilder<DA = Missing, LG = Missing> {
    on_input_plugins: Vec<Box<OnInputPlugin>>,
    before_prediction_plugins: Vec<Box<BeforePredictionPlugin>>,
    on_stream_event_plugins: Vec<Box<OnStreamEventPlugin>>,
    after_prediction_plugins: Vec<Box<AfterPredictionPlugin>>,
    on_tool_calls_plugins: Vec<Box<OnToolCallsPlugin>>,
    on_tool_result_plugins: Vec<Box<OnToolResultPlugin>>,
    on_error_plugins: Vec<Box<OnErrorPlugin>>,
    epoch_interceptors: Vec<Box<EpochInterceptorPlugin>>,
    subroutine_handler: Option<Box<SubRoutineHandlerPlugin>>,
    subroutine_hydrator: Option<Box<SubRoutineHydratorPlugin>>,
    consistency_verifier: Option<Box<ConsistencyVerifierPlugin>>,
    decision_aggregator: Option<Box<DecisionAggregatorPlugin>>,
    hook_effect_constraint: Option<Box<dyn HookEffectConstraint>>,
    cycle_rollback_policy: Option<Box<CycleRollbackPolicyPlugin>>,
    cow_budget_policy: Option<Box<CowBudgetPolicyPlugin>>,
    subroutine_lifecycle_guard: Option<Box<SubRoutineLifecycleGuardPlugin>>,
    governance_failover_handler: Option<Box<GovernanceFailoverHandlerPlugin>>,
    default_tool_timeout_ms: u64,
    _phantom: std::marker::PhantomData<(DA, LG)>,
}

impl Default for BriocheEngineBuilder<Missing, Missing> {
    fn default() -> Self {
        Self::new()
    }
}

impl BriocheEngineBuilder<Missing, Missing> {
    /// Create a new builder with no hooks or governance traits.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Core-BuilderTypeState
    pub fn new() -> Self {
        Self {
            on_input_plugins: Vec::new(),
            before_prediction_plugins: Vec::new(),
            on_stream_event_plugins: Vec::new(),
            after_prediction_plugins: Vec::new(),
            on_tool_calls_plugins: Vec::new(),
            on_tool_result_plugins: Vec::new(),
            on_error_plugins: Vec::new(),
            epoch_interceptors: Vec::new(),
            subroutine_handler: None,
            subroutine_hydrator: None,
            consistency_verifier: None,
            decision_aggregator: None,
            hook_effect_constraint: None,
            cycle_rollback_policy: None,
            cow_budget_policy: None,
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
            on_input_plugins: self.on_input_plugins,
            before_prediction_plugins: self.before_prediction_plugins,
            on_stream_event_plugins: self.on_stream_event_plugins,
            after_prediction_plugins: self.after_prediction_plugins,
            on_tool_calls_plugins: self.on_tool_calls_plugins,
            on_tool_result_plugins: self.on_tool_result_plugins,
            on_error_plugins: self.on_error_plugins,
            epoch_interceptors: self.epoch_interceptors,
            subroutine_handler: self.subroutine_handler,
            subroutine_hydrator: self.subroutine_hydrator,
            consistency_verifier: self.consistency_verifier,
            decision_aggregator: self.decision_aggregator,
            hook_effect_constraint: self.hook_effect_constraint,
            cycle_rollback_policy: self.cycle_rollback_policy,
            cow_budget_policy: self.cow_budget_policy,
            subroutine_lifecycle_guard: self.subroutine_lifecycle_guard,
            governance_failover_handler: self.governance_failover_handler,
            default_tool_timeout_ms: self.default_tool_timeout_ms,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Register an input capability plugin.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-TraitAtomic, I-Core-PluginOrder
    pub fn with_on_input(mut self, plugin: Box<OnInputPlugin>) -> Self {
        self.on_input_plugins.push(plugin);
        self
    }

    /// Register a before-prediction capability plugin.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-TraitAtomic, I-Core-PluginOrder
    pub fn with_before_prediction(mut self, plugin: Box<BeforePredictionPlugin>) -> Self {
        self.before_prediction_plugins.push(plugin);
        self
    }

    /// Register a stream-event capability plugin.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-TraitAtomic, I-Core-StreamNoBranch
    pub fn with_on_stream_event(mut self, plugin: Box<OnStreamEventPlugin>) -> Self {
        self.on_stream_event_plugins.push(plugin);
        self
    }

    /// Register an after-prediction capability plugin.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-TraitAtomic, I-Core-PluginOrder
    pub fn with_after_prediction(mut self, plugin: Box<AfterPredictionPlugin>) -> Self {
        self.after_prediction_plugins.push(plugin);
        self
    }

    /// Register a tool-call capability plugin.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-TraitAtomic, I-Core-ActiveToolCall
    pub fn with_on_tool_calls(mut self, plugin: Box<OnToolCallsPlugin>) -> Self {
        self.on_tool_calls_plugins.push(plugin);
        self
    }

    /// Register a tool-result capability plugin.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-TraitAtomic, I-Core-ActiveToolCall
    pub fn with_on_tool_result(mut self, plugin: Box<OnToolResultPlugin>) -> Self {
        self.on_tool_result_plugins.push(plugin);
        self
    }

    /// Register an error capability plugin.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-TraitAtomic, I-Gov-Failover
    pub fn with_on_error(mut self, plugin: Box<OnErrorPlugin>) -> Self {
        self.on_error_plugins.push(plugin);
        self
    }

    /// Add an `EpochInterceptor`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Comp-Epoch-First
    pub fn with_epoch_interceptor(mut self, interceptor: Box<EpochInterceptorPlugin>) -> Self {
        self.epoch_interceptors.push(interceptor);
        self
    }

    /// Inject a `SubRoutineHandler`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Comp-Epoch-Subroutine
    pub fn with_subroutine_handler(mut self, handler: Box<SubRoutineHandlerPlugin>) -> Self {
        self.subroutine_handler = Some(handler);
        self
    }

    /// Inject a `SubRoutineHydrator`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
    pub fn with_subroutine_hydrator(mut self, hydrator: Box<SubRoutineHydratorPlugin>) -> Self {
        self.subroutine_hydrator = Some(hydrator);
        self
    }

    /// Inject a `ConsistencyVerifier`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-Rebuild-Barrier
    pub fn with_consistency_verifier(mut self, verifier: Box<ConsistencyVerifierPlugin>) -> Self {
        self.consistency_verifier = Some(verifier);
        self
    }

    /// Inject a `HookEffectConstraint`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
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
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-Rollback-BestEffort
    pub fn with_cycle_rollback_policy(mut self, policy: Box<CycleRollbackPolicyPlugin>) -> Self {
        self.cycle_rollback_policy = Some(policy);
        self
    }

    /// Inject a `CowBudgetPolicy`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-CowBudget-Adaptative
    pub fn with_cow_budget_policy(mut self, policy: Box<CowBudgetPolicyPlugin>) -> Self {
        self.cow_budget_policy = Some(policy);
        self
    }

    /// Inject a `GovernanceFailoverHandler`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-Failover
    pub fn with_governance_failover_handler(
        mut self,
        handler: Box<GovernanceFailoverHandlerPlugin>,
    ) -> Self {
        self.governance_failover_handler = Some(handler);
        self
    }

    /// Set the default tool timeout applied when a descriptor omits `timeout_ms`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Core-ActiveToolCall
    pub fn with_default_tool_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.default_tool_timeout_ms = timeout_ms;
        self
    }
}

impl<LG> BriocheEngineBuilder<Missing, LG> {
    /// Inject a `DecisionAggregator` (mandatory).
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-Decision-Required
    pub fn with_decision_aggregator(
        mut self,
        aggregator: Box<DecisionAggregatorPlugin>,
    ) -> BriocheEngineBuilder<Present, LG> {
        self.decision_aggregator = Some(aggregator);
        self.change_type()
    }
}

impl<LG> BriocheEngineBuilder<Present, LG> {
    /// Override the `DecisionAggregator`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-Decision-Required
    pub fn with_decision_aggregator(mut self, aggregator: Box<DecisionAggregatorPlugin>) -> Self {
        self.decision_aggregator = Some(aggregator);
        self
    }
}

impl<DA> BriocheEngineBuilder<DA, Missing> {
    /// Inject a `SubRoutineLifecycleGuard` (mandatory).
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
    pub fn with_subroutine_lifecycle_guard(
        mut self,
        guard: Box<SubRoutineLifecycleGuardPlugin>,
    ) -> BriocheEngineBuilder<DA, Present> {
        self.subroutine_lifecycle_guard = Some(guard);
        self.change_type()
    }
}

impl<DA> BriocheEngineBuilder<DA, Present> {
    /// Override the `SubRoutineLifecycleGuard`.
    ///
    ///
    /// # Complexity
    /// O(1). Updates builder state without iterating hooks.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
    pub fn with_subroutine_lifecycle_guard(
        mut self,
        guard: Box<SubRoutineLifecycleGuardPlugin>,
    ) -> Self {
        self.subroutine_lifecycle_guard = Some(guard);
        self
    }
}

impl BriocheEngineBuilder<Present, Present> {
    /// Build the `BriocheEngine`.
    ///
    ///
    /// # Complexity
    /// O(p log p) where p is the total registered hook count. Builds the
    /// routing table by sorting each capability vector.
    ///
    /// # Panics
    /// Never panics.
    /// Refs: I-Core-BuilderTypeState, I-Gov-Decision-Required
    pub fn build(self) -> BriocheEngine {
        let routing_table = UnifiedRoutingTable::from_hooks(
            &self.on_input_plugins,
            &self.before_prediction_plugins,
            &self.on_stream_event_plugins,
            &self.after_prediction_plugins,
            &self.on_tool_calls_plugins,
            &self.on_tool_result_plugins,
            &self.on_error_plugins,
        );

        let cow_budget_policy = self.cow_budget_policy;
        let mut cycle_rollback_policy = self.cycle_rollback_policy;
        if let Some(rollback) = cycle_rollback_policy.as_mut()
            && let Some(policy) = cow_budget_policy
        {
            rollback.set_cow_budget_policy(policy);
        }

        BriocheEngine {
            router: PluginRouter {
                on_input_plugins: self.on_input_plugins,
                before_prediction_plugins: self.before_prediction_plugins,
                on_stream_event_plugins: self.on_stream_event_plugins,
                after_prediction_plugins: self.after_prediction_plugins,
                on_tool_calls_plugins: self.on_tool_calls_plugins,
                on_tool_result_plugins: self.on_tool_result_plugins,
                on_error_plugins: self.on_error_plugins,
                routing_table,
            },
            governance: crate::engine::GovernanceKernel {
                epoch_interceptors: self.epoch_interceptors,
                subroutine_handler: self.subroutine_handler,
                subroutine_hydrator: self.subroutine_hydrator,
                consistency_verifier: self.consistency_verifier,
                decision_aggregator: self.decision_aggregator,
                hook_effect_constraint: self.hook_effect_constraint,
                cycle_rollback_policy,
                subroutine_lifecycle_guard: self.subroutine_lifecycle_guard,
                governance_failover_handler: self.governance_failover_handler,
                default_tool_timeout_ms: self.default_tool_timeout_ms,
            },
            routines: crate::engine::RoutineManager::new(),
        }
    }
}
