//! Book I — The Core Book: Plugin hook evaluation.
//!
//! Canonical implementation of plugin route iteration with snapshot injection,
//! COW rollback, and uniform error collection.
//!
//! ## Invariants upheld
//! - I-Core-StreamNoBranch: Iterates pre-routed indices directly.
//! - I-Gov-Rollback-BestEffort: Rollback per plugin via `CycleRollbackPolicy`.
//! - I-Core-PluginOrder: Total order via `(priority, name)`.
//!
//! Refs: docs/SPECS.md §4.2

use super::{BriocheEngine, InputResult};
use crate::types::InconsistencySource;
use crate::{
    BriocheError, BriochePlugin, Effect, EngineInput, ErrorCode, ErrorDetail, PluginError,
    PluginResult, PluginSource, PolicyDecision, Session,
};

impl BriocheEngine {
    /// Evaluate a pre-routed plugin hook with snapshot injection, rollback,
    /// and uniform error collection.
    ///
    /// `hook` receives `(plugin, session)` and returns a `PluginResult<R>`.
    /// `on_ok` is called for each successful result.
    ///
    /// Returns a vector of `(plugin_name, error)` pairs for any plugin
    /// failures. The caller decides how to materialize these into effects.
    ///
    /// Snapshot is injected once before the loop. Rollback is applied
    /// per-plugin. This is the single canonical implementation of the
    /// iteration pattern; no caller may replicate it.
    ///
    /// ## Architectural Note
    /// The `hook` closure takes `&dyn BriochePlugin`. PHILOSOPHY.md §1
    /// discourages vtables, but the plugin container stores heterogeneous
    /// concrete types (`Vec<Box<dyn BriochePlugin>>`). Dispatch itself is
    /// pre-routed via `UnifiedRoutingTable` (O(1) index lookup); the vtable
    /// is only used for the actual heterogeneous method call after the
    /// route has been resolved. This is a documented, bounded indirection
    /// rather than a dynamic dispatch on the hot-path routing decision.
    ///
    /// # Complexity
    /// O(p) where p = route length. One snapshot insertion, one rollback
    /// per plugin.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Gov-Rollback-BestEffort
    /// # Panics
    /// Never panics. Errors are returned as `Result::Err`.
    pub(crate) fn eval_route<R>(
        &mut self,
        session: &mut Session,
        route: &[usize],
        mut hook: impl FnMut(&dyn BriochePlugin, &mut Session) -> PluginResult<R>,
        mut on_ok: impl FnMut(R),
    ) -> Vec<(&'static str, PluginError)> {
        session.extensions.insert(session.snapshot());
        let mut errors = Vec::new();
        for &idx in route {
            let name = match self.router.plugins.get(idx) {
                Some(p) => p.name(),
                None => {
                    errors.push((
                        "<invalid_index>",
                        PluginError::Fatal {
                            plugin_name: "<invalid_index>".into(),
                            message: format!("plugin index {idx} out of bounds"),
                        },
                    ));
                    continue;
                }
            };
            let result = self.with_rollback(session, name, |engine, session| {
                match engine.router.plugins.get(idx) {
                    Some(plugin) => hook(plugin.as_ref(), session),
                    None => Err(PluginError::Fatal {
                        plugin_name: "<invalid_index>".into(),
                        message: format!("plugin index {idx} out of bounds"),
                    }),
                }
            });
            match result {
                Ok(r) => on_ok(r),
                Err(err) => errors.push((name, err)),
            }
        }
        errors
    }

    /// Evaluate the `after_prediction` route.
    ///
    /// Called after the LLM prediction completes (before tool execution
    /// or transition to `Idle`). Collects `PluginFault` effects for any
    /// plugin errors but does not short-circuit.
    ///
    /// Refs: I-Core-PluginOrder, I-Core-StreamNoBranch
    /// Complexity: O(p) where p = plugins on route_after_prediction.
    /// # Panics
    /// Never panics.
    pub(crate) fn eval_after_prediction(
        &mut self,
        session: &mut Session,
        effects: &mut Vec<Effect>,
    ) {
        let route = self.router.routing_table.route_after_prediction.clone();
        let faults = self.eval_route(
            session,
            &route,
            |plugin, session| plugin.after_prediction(&mut session.extensions),
            |_ok| {},
        );
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }
    }

    /// Evaluate the `on_input` route.
    ///
    /// `OverrideTransition` from the first plugin wins; subsequent ones are
    /// logged as superseded. `Block` short-circuits immediately.
    ///
    /// Refs: I-Core-PluginOrder, I-Gov-Decision-Required
    /// Complexity: O(p) where p = plugins on route_on_input.
    /// # Panics
    /// Panics only if an index is out of bounds; callers must validate lengths.
    pub(crate) fn eval_on_input(
        &mut self,
        session: &mut Session,
        input: &EngineInput,
    ) -> InputResult {
        let mut accumulated = Vec::new();
        let mut override_transition: Option<(Vec<Effect>, PluginSource)> = None;

        session.extensions.insert(session.snapshot());
        let route = self.router.routing_table.route_on_input.clone();
        for &idx in &route {
            let Some(plugin) = self.router.plugins.get(idx) else {
                accumulated.push(Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    detail: ErrorDetail::StateInconsistent {
                        source: InconsistencySource::Kernel {
                            module: "hooks::eval_on_input".to_string(),
                        },
                    },
                });
                continue;
            };
            let name = plugin.name();

            let decision = self.with_rollback(session, name, |engine, session| {
                match engine.router.plugins.get(idx) {
                    Some(plugin) => plugin.as_ref().on_input(input, &mut session.extensions),
                    None => Err(PluginError::Fatal {
                        plugin_name: "<invalid_index>".into(),
                        message: format!("plugin index {idx} out of bounds"),
                    }),
                }
            });

            match decision {
                Ok(PolicyDecision::Allow) => {}
                Ok(PolicyDecision::Block { reason }) => {
                    return InputResult::Block {
                        detail: ErrorDetail::HookConstraintFailed {
                            reason: reason.clone(),
                        },
                    };
                }
                Ok(PolicyDecision::MutateHistory(edits)) => {
                    if let Err(err) = session.apply_history_edits(&edits) {
                        accumulated.push(Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            detail: ErrorDetail::HookConstraintFailed {
                                reason: err.to_string(),
                            },
                        });
                    }
                }
                Ok(PolicyDecision::RequestEffect(eff)) => {
                    accumulated.push(eff);
                }
                Ok(PolicyDecision::OverrideTransition(effects)) => {
                    let source = PluginSource(name.into());
                    if override_transition.is_none() {
                        override_transition = Some((effects, source));
                    } else {
                        self.log_superseded_transition(
                            session,
                            &source,
                            &PolicyDecision::OverrideTransition(effects),
                        );
                    }
                }
                Err(err) => {
                    accumulated.push(Self::plugin_fault(name, err));
                }
            }
        }

        if let Some((effects, source)) = override_transition {
            InputResult::OverrideTransition(effects, source)
        } else if accumulated.is_empty() {
            InputResult::Allow
        } else {
            InputResult::Accumulated(accumulated)
        }
    }

    /// Invoke the `on_tool_calls` hook on all pre-routed plugins.
    ///
    /// Plugins mutate `timeout_ms` and other fields in place.
    ///
    /// Refs: I-Core-PluginOrder, I-Core-ActiveToolCall
    /// Complexity: O(p) where p = plugins on route_on_tool_calls.
    /// # Panics
    /// Never panics. Errors are returned as `Result::Err`.
    pub(crate) fn handle_tool_calls(
        &mut self,
        session: &mut Session,
        descriptors: &mut Vec<crate::ToolCallDescriptor>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        let route = self.router.routing_table.route_on_tool_calls.clone();
        let faults = self.eval_route(
            session,
            &route,
            |plugin, session| plugin.on_tool_calls(descriptors, &mut session.extensions),
            |_ok| {},
        );
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }
        Ok(())
    }
}
