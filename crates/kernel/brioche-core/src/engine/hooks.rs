//! Book I — The Core Book: Plugin hook evaluation.
//!
//! Canonical implementation of capability route iteration with COW rollback
//! and uniform error collection.
//!
//! Refs: docs/SPECS.md §4.2

use super::{BriocheEngine, InputResult};
use crate::types::InconsistencySource;
use crate::{
    AfterPredictionPlugin, BeforePredictionPlugin, BriocheError, CycleRollbackPolicyPlugin, Effect,
    EngineInput, ErrorCode, ErrorDetail, OnErrorPlugin, OnInputPlugin, OnStreamEventPlugin,
    OnToolCallsPlugin, OnToolResultPlugin, PluginError, PluginResult, PluginSource, PolicyDecision,
    Session,
};

/// Compact hook indices used for `HookEffectConstraint` validation.
pub(crate) const HOOK_INDEX_ON_INPUT: u8 = 0;
/// Compact hook index for `before_prediction`.
pub(crate) const HOOK_INDEX_BEFORE_PREDICTION: u8 = 1;
/// Compact hook index for `on_stream_event`.
pub(crate) const HOOK_INDEX_ON_STREAM_EVENT: u8 = 2;
/// Compact hook index for `on_error`.
pub(crate) const HOOK_INDEX_ON_ERROR: u8 = 6;

pub(crate) trait NamedHook {
    fn hook_name(&self) -> &'static str;
}

impl NamedHook for OnInputPlugin {
    fn hook_name(&self) -> &'static str {
        OnInputPlugin::name(self)
    }
}

impl NamedHook for AfterPredictionPlugin {
    fn hook_name(&self) -> &'static str {
        AfterPredictionPlugin::name(self)
    }
}

impl NamedHook for BeforePredictionPlugin {
    fn hook_name(&self) -> &'static str {
        BeforePredictionPlugin::name(self)
    }
}

impl NamedHook for OnStreamEventPlugin {
    fn hook_name(&self) -> &'static str {
        OnStreamEventPlugin::name(self)
    }
}

impl NamedHook for OnToolCallsPlugin {
    fn hook_name(&self) -> &'static str {
        OnToolCallsPlugin::name(self)
    }
}

impl NamedHook for OnToolResultPlugin {
    fn hook_name(&self) -> &'static str {
        OnToolResultPlugin::name(self)
    }
}

impl NamedHook for OnErrorPlugin {
    fn hook_name(&self) -> &'static str {
        OnErrorPlugin::name(self)
    }
}

impl BriocheEngine {
    /// Evaluate a pre-routed capability hook with rollback.
    ///
    /// # Complexity
    /// O(p) where p = route length. No route allocation.
    ///
    /// # Panics
    /// Never panics. Invalid route indices are returned as plugin errors.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Gov-Rollback-BestEffort
    pub(crate) fn eval_route<T, R>(
        plugins: &[Box<T>],
        rollback_policy: &mut Option<Box<CycleRollbackPolicyPlugin>>,
        session: &mut Session,
        hook_name: &'static str,
        route: &[usize],
        mut hook: impl FnMut(&T, &mut Session) -> PluginResult<R>,
        mut on_ok: impl FnMut(R),
    ) -> Vec<(&'static str, PluginError)>
    where
        T: NamedHook + ?Sized,
    {
        let mut errors = Vec::new();
        for &idx in route {
            let name = match plugins.get(idx) {
                Some(plugin) => plugin.hook_name(),
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
            let result =
                Self::with_rollback(rollback_policy, session, hook_name, |session| match plugins
                    .get(idx)
                {
                    Some(plugin) => hook(plugin.as_ref(), session),
                    None => Err(PluginError::Fatal {
                        plugin_name: "<invalid_index>".into(),
                        message: format!("plugin index {idx} out of bounds"),
                    }),
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
    /// # Complexity
    /// O(p) where p = after-prediction route length.
    ///
    /// # Panics
    /// Never panics. Plugin errors are materialized as effects.
    ///
    /// Refs: I-Core-PluginOrder, I-Core-StreamNoBranch
    pub(crate) fn eval_after_prediction(
        &mut self,
        session: &mut Session,
        effects: &mut Vec<Effect>,
    ) {
        let faults = {
            let plugins = &self.router.after_prediction_plugins;
            let route = &self.router.routing_table.route_after_prediction;
            let rollback_policy = &mut self.governance.cycle_rollback_policy;
            Self::eval_route(
                plugins,
                rollback_policy,
                session,
                "after_prediction",
                route,
                |plugin, session| plugin.after_prediction(&mut session.extensions),
                |_ok| {},
            )
        };
        let on_error_effects = self.eval_on_error(session, &faults);
        effects.extend(on_error_effects);
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }
    }

    /// Evaluate the `on_input` route.
    ///
    /// # Complexity
    /// O(p) where p = input route length.
    ///
    /// # Panics
    /// Never panics. Invalid route indices become error effects.
    ///
    /// Refs: I-Core-PluginOrder, I-Gov-Decision-Required
    pub(crate) fn eval_on_input(
        &mut self,
        session: &mut Session,
        input: &EngineInput,
    ) -> InputResult {
        let plugins = &self.router.on_input_plugins;
        let route = &self.router.routing_table.route_on_input;

        let mut accumulated = Vec::new();
        let mut override_transition: Option<(Vec<Effect>, PluginSource)> = None;
        let mut faults = Vec::new();

        for &idx in route {
            let Some(plugin) = plugins.get(idx) else {
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
            let name = plugin.hook_name();
            let decision = Self::with_rollback(
                &mut self.governance.cycle_rollback_policy,
                session,
                "on_input",
                |session| match plugins.get(idx) {
                    Some(plugin) => plugin.as_ref().on_input(input, &mut session.extensions),
                    None => Err(PluginError::Fatal {
                        plugin_name: "<invalid_index>".into(),
                        message: format!("plugin index {idx} out of bounds"),
                    }),
                },
            );

            match decision {
                Ok(PolicyDecision::Allow) => {}
                Ok(PolicyDecision::Block { reason }) => {
                    return InputResult::Block {
                        detail: ErrorDetail::HookConstraintFailed { reason },
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
                    faults.push((name, err));
                }
            }
        }

        let on_error_effects = self.eval_on_error(session, &faults);
        accumulated.extend(on_error_effects);
        for (name, err) in faults {
            accumulated.push(Self::plugin_fault(name, err));
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
    /// # Complexity
    /// O(p + c) where p = tool-call route length and c = descriptors.
    ///
    /// # Panics
    /// Never panics. Plugin faults are appended as effects.
    ///
    /// Refs: I-Core-PluginOrder, I-Core-ActiveToolCall
    pub(crate) fn handle_tool_calls(
        &mut self,
        session: &mut Session,
        descriptors: &mut Vec<crate::ToolCallDescriptor>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        let faults = {
            let plugins = &self.router.on_tool_calls_plugins;
            let route = &self.router.routing_table.route_on_tool_calls;
            let rollback_policy = &mut self.governance.cycle_rollback_policy;
            Self::eval_route(
                plugins,
                rollback_policy,
                session,
                "on_tool_calls",
                route,
                |plugin, session| plugin.on_tool_calls(descriptors, &mut session.extensions),
                |_ok| {},
            )
        };
        let on_error_effects = self.eval_on_error(session, &faults);
        effects.extend(on_error_effects);
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }
        Ok(())
    }

    /// Evaluate the `on_error` hook for intercepted plugin faults.
    ///
    /// # Complexity
    /// O(p * f) where p = error handlers and f = faults.
    ///
    /// # Panics
    /// Never panics. Handler failures become plugin-fault effects.
    ///
    /// Refs: I-Core-PluginOrder, I-Gov-ErrorHandling
    pub(crate) fn eval_on_error(
        &mut self,
        session: &mut Session,
        faults: &[(&'static str, PluginError)],
    ) -> Vec<Effect> {
        if faults.is_empty() {
            return Vec::new();
        }

        let route = &self.router.routing_table.route_on_error;
        if route.is_empty() {
            return Vec::new();
        }

        let mut effects = Vec::new();
        for &idx in route {
            let Some(plugin) = self.router.on_error_plugins.get(idx) else {
                continue;
            };
            let name = plugin.hook_name();

            for (_fault_plugin, error) in faults {
                let decision = Self::with_rollback(
                    &mut self.governance.cycle_rollback_policy,
                    session,
                    "on_error",
                    |session| match self.router.on_error_plugins.get(idx) {
                        Some(plugin) => plugin.as_ref().on_error(error, &mut session.extensions),
                        None => Err(PluginError::Fatal {
                            plugin_name: "<invalid_index>".into(),
                            message: format!("plugin index {idx} out of bounds"),
                        }),
                    },
                );

                match decision {
                    Ok(PolicyDecision::RequestEffect(eff)) => {
                        effects.push(eff);
                    }
                    Ok(PolicyDecision::OverrideTransition(ov)) => {
                        effects.extend(ov);
                    }
                    Ok(_) => {}
                    Err(err) => {
                        effects.push(Self::plugin_fault(name, err));
                    }
                }
            }
        }

        self.validate_hook_effects(HOOK_INDEX_ON_ERROR, "on_error", &mut effects);
        effects
    }
}
