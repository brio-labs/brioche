//! `PluginBuilder` — ergonomic wrapper around `BriocheEngineBuilder`.
//!
//! Provides a fluent API for constructing engines with atomic capability
//! plugins and governance traits.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.2

use brioche_core::{
    AfterPredictionPlugin, BeforePredictionPlugin, BriocheEngine, BriocheEngineBuilder,
    ConsistencyVerifierPlugin, CycleRollbackPolicyPlugin, DecisionAggregatorPlugin,
    EpochInterceptorPlugin, GovernanceFailoverHandlerPlugin, HookEffectConstraint, OnErrorPlugin,
    OnInputPlugin, OnStreamEventPlugin, OnToolCallsPlugin, OnToolResultPlugin, Present, Session,
    SubRoutineHandlerPlugin, SubRoutineHydratorPlugin, SubRoutineLifecycleGuardPlugin,
};
use brioche_governance_default::GovernanceProfile;

/// Ergonomic builder for constructing a `BriocheEngine` with capability plugins.
///
/// Refs: I-Gov-Profile-Agnostic, I-Gov-TraitAtomic
pub struct PluginBuilder {
    inner: BriocheEngineBuilder<Present, Present>,
}

impl Default for PluginBuilder {
    fn default() -> Self {
        Self::standard()
    }
}

impl PluginBuilder {
    /// Start a builder with the `Standard` governance profile.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn standard() -> Self {
        let builder = BriocheEngineBuilder::new();
        let builder = GovernanceProfile::Standard.apply(builder);
        Self { inner: builder }
    }

    /// Start a builder with the `Permissive` governance profile.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn permissive() -> Self {
        let builder = BriocheEngineBuilder::new();
        let builder = GovernanceProfile::Permissive.apply(builder);
        Self { inner: builder }
    }

    /// Start a builder with the `Strict` governance profile.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn strict() -> Self {
        let builder = BriocheEngineBuilder::new();
        let builder = GovernanceProfile::Strict.apply(builder);
        Self { inner: builder }
    }

    /// Add an input capability plugin.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_on_input(mut self, plugin: Box<OnInputPlugin>) -> Self {
        self.inner = self.inner.with_on_input(plugin);
        self
    }

    /// Add a before-prediction capability plugin.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_before_prediction(mut self, plugin: Box<BeforePredictionPlugin>) -> Self {
        self.inner = self.inner.with_before_prediction(plugin);
        self
    }

    /// Add a stream-event capability plugin.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_on_stream_event(mut self, plugin: Box<OnStreamEventPlugin>) -> Self {
        self.inner = self.inner.with_on_stream_event(plugin);
        self
    }

    /// Add an after-prediction capability plugin.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_after_prediction(mut self, plugin: Box<AfterPredictionPlugin>) -> Self {
        self.inner = self.inner.with_after_prediction(plugin);
        self
    }

    /// Add a tool-call capability plugin.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_on_tool_calls(mut self, plugin: Box<OnToolCallsPlugin>) -> Self {
        self.inner = self.inner.with_on_tool_calls(plugin);
        self
    }

    /// Add a tool-result capability plugin.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_on_tool_result(mut self, plugin: Box<OnToolResultPlugin>) -> Self {
        self.inner = self.inner.with_on_tool_result(plugin);
        self
    }

    /// Add an error capability plugin.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_on_error(mut self, plugin: Box<OnErrorPlugin>) -> Self {
        self.inner = self.inner.with_on_error(plugin);
        self
    }

    /// Set the default tool timeout in milliseconds.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_default_tool_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.inner = self.inner.with_default_tool_timeout_ms(timeout_ms);
        self
    }

    /// Override the `EpochInterceptor`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_epoch_interceptor(mut self, interceptor: Box<EpochInterceptorPlugin>) -> Self {
        self.inner = self.inner.with_epoch_interceptor(interceptor);
        self
    }

    /// Override the `SubRoutineHandler`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_subroutine_handler(mut self, handler: Box<SubRoutineHandlerPlugin>) -> Self {
        self.inner = self.inner.with_subroutine_handler(handler);
        self
    }

    /// Override the `SubRoutineHydrator`.
    ///
    /// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
    pub fn with_subroutine_hydrator(mut self, hydrator: Box<SubRoutineHydratorPlugin>) -> Self {
        self.inner = self.inner.with_subroutine_hydrator(hydrator);
        self
    }

    /// Override the `ConsistencyVerifier`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_consistency_verifier(mut self, verifier: Box<ConsistencyVerifierPlugin>) -> Self {
        self.inner = self.inner.with_consistency_verifier(verifier);
        self
    }

    /// Override the `DecisionAggregator`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_decision_aggregator(mut self, aggregator: Box<DecisionAggregatorPlugin>) -> Self {
        self.inner = self.inner.with_decision_aggregator(aggregator);
        self
    }

    /// Override the `HookEffectConstraint`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_hook_effect_constraint(
        mut self,
        constraint: Box<dyn HookEffectConstraint>,
    ) -> Self {
        self.inner = self.inner.with_hook_effect_constraint(constraint);
        self
    }

    /// Override the `CycleRollbackPolicy`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_cycle_rollback_policy(mut self, policy: Box<CycleRollbackPolicyPlugin>) -> Self {
        self.inner = self.inner.with_cycle_rollback_policy(policy);
        self
    }

    /// Override the `SubRoutineLifecycleGuard`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_subroutine_lifecycle_guard(
        mut self,
        guard: Box<SubRoutineLifecycleGuardPlugin>,
    ) -> Self {
        self.inner = self.inner.with_subroutine_lifecycle_guard(guard);
        self
    }

    /// Override the `GovernanceFailoverHandler`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn with_governance_failover_handler(
        mut self,
        handler: Box<GovernanceFailoverHandlerPlugin>,
    ) -> Self {
        self.inner = self.inner.with_governance_failover_handler(handler);
        self
    }

    /// Build the `BriocheEngine`.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn build(self) -> BriocheEngine {
        self.inner.build()
    }

    /// Build the engine and create a new `Session` in one step.
    ///
    /// Refs: I-Gov-Profile-Agnostic
    pub fn build_with_session(self, session_id: impl Into<String>) -> (BriocheEngine, Session) {
        let engine = self.build();
        let session = Session::new(session_id);
        (engine, session)
    }
}
