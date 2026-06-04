use crate::{
    ActiveToolCall, AgentState, Effect, EngineInput, EpochAction, ErrorCode, ErrorDetail,
    PluginError, Session,
};

use super::BriocheEngine;
use super::types::PreTransitionState;

impl BriocheEngine {
    /// Capture pre-transition sub-routine state.
    ///
    /// This must run before any state mutation in `transition()` so that
    /// lifecycle guards can detect whether the transition exits a sub-routine.
    ///
    /// Refs: I-Gov-SubRoutineLifecycle-Guard
    pub(crate) fn capture_pre_transition_state(&self, session: &Session) -> PreTransitionState {
        let was_subroutine = matches!(session.state, AgentState::SubRoutine(_));
        let handle = match &session.state {
            AgentState::SubRoutine(h) => Some(h.clone()),
            _ => None,
        };
        PreTransitionState {
            was_subroutine,
            handle,
        }
    }

    /// Apply `EpochInterceptor` if configured.
    ///
    /// Returns `Some(())` when the interceptor produces a terminal action
    /// (Block), in which case `effects` has been populated and the caller
    /// should return early.
    ///
    /// Refs: I-Comp-Epoch-First
    pub(crate) fn apply_epoch_interceptor(
        &mut self,
        session: &mut Session,
        input: &EngineInput,
        effects: &mut Vec<Effect>,
    ) -> Option<()> {
        let interceptor = self.governance.epoch_interceptor.as_ref()?;

        match interceptor.intercept_epoch(input, &mut session.extensions) {
            Ok(EpochAction::Block { reason }) => {
                effects.push(Effect::Error {
                    code: ErrorCode::EpochMismatch,
                    detail: ErrorDetail::Generic(reason),
                });
                effects.push(Effect::SystemIdle);
                Some(())
            }
            Ok(EpochAction::Proceed) => None,
            Err(err) => {
                effects.push(Self::plugin_fault("epoch_interceptor", err));
                None
            }
        }
    }

    /// Apply `SubRoutineHandler` if configured and session is in sub-routine state.
    ///
    /// Returns `Some(())` when the handler produces terminal effects,
    /// in which case the caller should return early.
    ///
    /// Refs: I-Comp-Epoch-Subroutine
    pub(crate) fn apply_subroutine_handler(
        &mut self,
        session: &mut Session,
        input: &EngineInput,
        effects: &mut Vec<Effect>,
    ) -> Option<()> {
        let handler = self.governance.subroutine_handler.as_ref()?;
        let handle = match &session.state {
            AgentState::SubRoutine(h) => h,
            _ => return None,
        };
        let child = self.routines.registry.get_mut(handle)?;

        match handler.handle_subroutine(session, child, input) {
            Ok(Some(sub_effects)) => {
                effects.extend(sub_effects);
                Some(())
            }
            Ok(None) => None,
            Err(err) => {
                effects.push(Self::plugin_fault("subroutine_handler", err));
                None
            }
        }
    }

    /// Wrap a single plugin hook invocation with COW rollback.
    ///
    /// The `cycle_rollback_policy` is temporarily moved into
    /// `session.extensions` before the hook and retrieved afterward.
    ///
    /// **Note on time instrumentation:** per-hook wall-clock timing is
    /// intentionally **not** performed in Core. `Instant::now()` is
    /// disallowed in Core by PHILOSOPHY.md §2.2 to preserve determinism.
    /// Time-based safety is provided by the Shell Runtime (`EngineWatchdog`).
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    pub(crate) fn with_rollback<R>(
        &mut self,
        session: &mut Session,
        _plugin_name: &'static str,
        f: impl FnOnce(&mut Self, &mut Session) -> R,
    ) -> R {
        let mut policy = self.governance.cycle_rollback_policy.take();
        if let Some(ref mut p) = policy {
            p.begin_hook();
        }

        if let Some(p) = policy {
            session.extensions.attach_rollback_policy(p);
        }

        let result = f(self, session);

        let mut policy = session.extensions.detach_rollback_policy();

        if let Some(ref mut p) = policy {
            p.commit_hook(&mut session.extensions);
        }

        self.governance.cycle_rollback_policy = policy;
        result
    }

    /// Build a `PluginFault` effect.
    ///
    /// Refs: I-Core-NoPanic
    pub(crate) fn plugin_fault(name: &str, error: PluginError) -> Effect {
        Effect::PluginFault {
            plugin_name: name.to_string(),
            error,
        }
    }

    /// Canonical conversion from `ToolCallDescriptor` to `ActiveToolCall`.
    ///
    /// Any descriptor missing `timeout_ms` receives `default_tool_timeout_ms`
    /// and an `Effect::Error(StateInconsistency)` is returned alongside the
    /// sealed calls.
    ///
    /// Refs: I-Core-ActiveToolCall, I-Core-NoPanic
    pub(crate) fn materialize_tool_calls(
        &self,
        descriptors: Vec<crate::ToolCallDescriptor>,
    ) -> (Vec<ActiveToolCall>, Option<Effect>) {
        super::trace::seal_tool_descriptors(descriptors, self.governance.default_tool_timeout_ms)
    }

    /// Append mechanism-level effects for the current session state.
    ///
    /// This is the single point where the kernel maps state to mandatory
    /// follow-up effects.
    ///
    /// Refs: I-Core-RetVecEffect, I-Core-ActiveToolCall
    pub(crate) fn append_state_effects(&self, session: &Session, effects: &mut Vec<Effect>) {
        effects.push(Effect::SaveSession);
        match session.state {
            AgentState::Predicting { .. } => {
                effects.push(Effect::CallLlmNetwork);
            }
            AgentState::ExecutingTools { .. } => {
                effects.push(Effect::ExecuteTools(session.active_tools.clone()));
            }
            AgentState::Idle => {
                effects.push(Effect::SystemIdle);
            }
            AgentState::SubRoutine(_) | AgentState::Failure => {
                // Terminal / delegated states emit no automatic follow-up effects.
            }
        }
    }

    /// Guarantee that `RebuildRoutes` occupies the last position in effects.
    ///
    /// If effects exist after `RebuildRoutes`, they are dropped and an
    /// `Effect::Error` is inserted to record the anomaly.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    pub(crate) fn ensure_rebuildroutes_last(effects: &mut Vec<Effect>) {
        let Some(pos) = effects
            .iter()
            .rposition(|e| matches!(e, Effect::RebuildRoutes))
        else {
            return;
        };

        let dropped = effects.len().saturating_sub(pos + 1);
        if dropped == 0 {
            return;
        }

        effects.insert(
            pos,
            Effect::Error {
                code: ErrorCode::StateInconsistency,
                detail: ErrorDetail::EffectsDroppedAfterRebuildRoutes { count: dropped },
            },
        );
        effects.truncate(pos + 2);
    }
}
