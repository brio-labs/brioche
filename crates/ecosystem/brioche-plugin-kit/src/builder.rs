//! `PluginBuilder` — ergonomic wrapper around `BriocheEngineBuilder`.
//!
//! Provides a fluent API for constructing engines with plugins and
//! governance traits, suitable for both production and test code.
//!
//! Refs: SPECS.md §Book IV

use brioche_core::{
    BriocheEngine, BriocheEngineBuilder, BriochePlugin, ConsistencyVerifier, CycleRollbackPolicy,
    DecisionAggregator, EpochInterceptor, GovernanceFailoverHandler, HookEffectConstraint, Present,
    Session, SubRoutineHandler, SubRoutineLifecycleGuard,
};
use brioche_governance_default::GovernanceProfile;

/// Ergonomic builder for constructing a `BriocheEngine` with plugins.
///
/// Wraps `BriocheEngineBuilder` and pre-wires mandatory governance traits
/// using the `Standard` governance profile by default.
///
/// # Example
/// ```ignore
/// let engine = PluginBuilder::standard()
///     .with_plugin(Box::new(MyPlugin))
///     .build();
/// ```
///
/// Refs: I-Gov-Profile-Agnostic
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
    /// This is the recommended starting point for most use cases.
    /// It injects all mandatory traits and sensible defaults.
    pub fn standard() -> Self {
        let builder = BriocheEngineBuilder::new();
        let builder = GovernanceProfile::Standard.apply(builder);
        Self { inner: builder }
    }

    /// Start a builder with the `Permissive` governance profile.
    ///
    /// Use for development, testing, and rapid prototyping.
    /// Minimal policy constraints; all optional traits are no-ops.
    pub fn permissive() -> Self {
        let builder = BriocheEngineBuilder::new();
        let builder = GovernanceProfile::Permissive.apply(builder);
        Self { inner: builder }
    }

    /// Start a builder with the `Strict` governance profile.
    ///
    /// Use for production deployments requiring maximum safety.
    /// All optional traits are active with conservative thresholds.
    pub fn strict() -> Self {
        let builder = BriocheEngineBuilder::new();
        let builder = GovernanceProfile::Strict.apply(builder);
        Self { inner: builder }
    }

    /// Add a plugin to the engine.
    pub fn with_plugin(mut self, plugin: Box<dyn BriochePlugin>) -> Self {
        self.inner = self.inner.with_plugin(plugin);
        self
    }

    /// Set the default tool timeout in milliseconds.
    pub fn with_default_tool_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.inner = self.inner.with_default_tool_timeout_ms(timeout_ms);
        self
    }

    /// Override the `EpochInterceptor`.
    pub fn with_epoch_interceptor(mut self, interceptor: Box<dyn EpochInterceptor>) -> Self {
        self.inner = self.inner.with_epoch_interceptor(interceptor);
        self
    }

    /// Override the `SubRoutineHandler`.
    pub fn with_subroutine_handler(mut self, handler: Box<dyn SubRoutineHandler>) -> Self {
        self.inner = self.inner.with_subroutine_handler(handler);
        self
    }

    /// Override the `ConsistencyVerifier`.
    pub fn with_consistency_verifier(mut self, verifier: Box<dyn ConsistencyVerifier>) -> Self {
        self.inner = self.inner.with_consistency_verifier(verifier);
        self
    }

    /// Override the `DecisionAggregator`.
    pub fn with_decision_aggregator(mut self, aggregator: Box<dyn DecisionAggregator>) -> Self {
        self.inner = self.inner.with_decision_aggregator(aggregator);
        self
    }

    /// Override the `HookEffectConstraint`.
    pub fn with_hook_effect_constraint(
        mut self,
        constraint: Box<dyn HookEffectConstraint>,
    ) -> Self {
        self.inner = self.inner.with_hook_effect_constraint(constraint);
        self
    }

    /// Override the `CycleRollbackPolicy`.
    pub fn with_cycle_rollback_policy(mut self, policy: Box<dyn CycleRollbackPolicy>) -> Self {
        self.inner = self.inner.with_cycle_rollback_policy(policy);
        self
    }

    /// Override the `SubRoutineLifecycleGuard`.
    pub fn with_subroutine_lifecycle_guard(
        mut self,
        guard: Box<dyn SubRoutineLifecycleGuard>,
    ) -> Self {
        self.inner = self.inner.with_subroutine_lifecycle_guard(guard);
        self
    }

    /// Override the `GovernanceFailoverHandler`.
    pub fn with_governance_failover_handler(
        mut self,
        handler: Box<dyn GovernanceFailoverHandler>,
    ) -> Self {
        self.inner = self.inner.with_governance_failover_handler(handler);
        self
    }

    /// Build the `BriocheEngine`.
    pub fn build(self) -> BriocheEngine {
        self.inner.build()
    }

    /// Build the engine and create a new `Session` in one step.
    ///
    /// Returns `(engine, session)` ready for `transition()` calls.
    pub fn build_with_session(self, session_id: impl Into<String>) -> (BriocheEngine, Session) {
        let engine = self.build();
        let session = Session::new(session_id);
        (engine, session)
    }
}
