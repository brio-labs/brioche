//! Book I — The Core Book: `EngineInput` dispatch mechanics.
//!
//! Routes each `EngineInput` variant to its handler. No policy decisions
//! are made here; all routing is pure mechanism.
//!
//! ## Invariants upheld
//! - I-Core-StreamNoBranch: Pre-routed dispatch via `UnifiedRoutingTable`.
//! - I-Core-RetVecEffect: Effects are appended to a mutable `Vec`.
//! - I-Core-ActiveToolCall: Tool descriptors are sealed before storage.
//!
//! Refs: docs/SPECS.md §4, §5

use super::BriocheEngine;
use crate::types::InconsistencySource;
use crate::{
    AgentState, BriocheError, ChatMessage, Effect, EngineInput, ErrorCode, ErrorDetail,
    PluginError, PolicyDecision, Session, StreamAction, StreamEvent, SubRoutineHandle, TaskId,
    ToolResultDTO,
};

impl BriocheEngine {
    /// Main dispatch — routes `EngineInput` to the appropriate handler.
    ///
    /// Appends effects into the provided buffer rather than returning a new
    /// `Vec`, eliminating per-transition allocations.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Core-RetVecEffect
    ///
    /// # Complexity
    /// O(1) dispatch + O(handler cost).
    ///
    /// # Panics
    /// Never panics. Errors are returned as `Result::Err`.
    pub(crate) fn dispatch_input(
        &mut self,
        session: &mut Session,
        input: &EngineInput,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        match input {
            EngineInput::UserMessage(content) => {
                self.dispatch_user_message(session, content, effects)
            }
            EngineInput::LlmStream(event) => self.dispatch_llm_stream(session, event, effects),
            EngineInput::ToolCallsResult {
                generation_id,
                results,
            } => self.dispatch_tool_calls_result(session, *generation_id, results, effects),
            EngineInput::RestoreSubRoutine { handle, head_blob } => {
                self.dispatch_restore_subroutine(session, handle, head_blob, effects)
            }
        }
    }

    /// Dispatch `UserMessage` input.
    ///
    /// Pushes the message into history, transitions to `Predicting`,
    /// evaluates the `before_prediction` hook route, and invokes the
    /// `DecisionAggregator`.
    ///
    /// # Complexity
    /// O(p + h) where p = plugins on `route_before_prediction`,
    /// h = history length (for the slice passed to plugins).
    ///
    /// Refs: I-Core-PluginOrder, I-Core-NoPanic, I-Gov-Decision-Required
    fn dispatch_user_message(
        &mut self,
        session: &mut Session,
        content: &str,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        session.history.push(ChatMessage::User {
            content: content.to_string(),
        });

        let generation_id = self.routines.next_generation_id;
        self.routines.next_generation_id += 1;

        session.push_state(AgentState::Predicting { generation_id })?;

        // before_prediction hook: collect decisions.
        let mut decisions: Vec<(PluginSource, PolicyDecision)> = Vec::new();
        let route = self.router.routing_table.route_before_prediction.clone();
        let faults = self.eval_route(
            session,
            "before_prediction",
            &route,
            |plugin, session| {
                let decision = plugin
                    .as_before_prediction()
                    .ok_or(PluginError::Fatal {
                        plugin_name: "<capability_missing>".into(),
                        message: "plugin missing BeforePrediction capability".into(),
                    })?
                    .before_prediction(&session.history, &mut session.extensions)?;
                Ok((PluginSource(plugin.name().into()), decision))
            },
            |entry| decisions.push(entry),
        );
        let on_error_effects = self.eval_on_error(session, &faults);
        effects.extend(on_error_effects);
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }
        if let Some(ref aggregator) = self.governance.decision_aggregator {
            match aggregator.aggregate_decisions(&decisions) {
                Ok(decision) => match decision {
                    PolicyDecision::Allow => {}
                    PolicyDecision::Block { reason } => {
                        effects.push(Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            detail: ErrorDetail::HookConstraintFailed {
                                reason: reason.clone(),
                            },
                        });
                        effects.push(Effect::SystemIdle);
                        return Ok(());
                    }
                    PolicyDecision::MutateHistory(edits) => {
                        session.apply_history_edits(&edits)?;
                    }
                    PolicyDecision::RequestEffect(eff) => {
                        let mut tmp = vec![eff];
                        self.validate_hook_effects(
                            crate::engine::hooks::HOOK_INDEX_BEFORE_PREDICTION,
                            "before_prediction",
                            &mut tmp,
                        );
                        effects.extend(tmp);
                    }
                    PolicyDecision::OverrideTransition(ov) => {
                        let mut tmp = ov;
                        self.validate_hook_effects(
                            crate::engine::hooks::HOOK_INDEX_BEFORE_PREDICTION,
                            "before_prediction",
                            &mut tmp,
                        );
                        effects.extend(tmp);
                        return Ok(());
                    }
                },
                Err(err) => {
                    effects.push(Self::plugin_fault("decision_aggregator", err));
                }
            }
        }

        self.append_state_effects(session, effects);

        Ok(())
    }

    /// Dispatch `LlmStream` input.
    ///
    /// Accumulates tool calls from `ToolCallStart` / `ToolArgumentChunk`
    /// events. When `ToolCallDone` is received, pending descriptors are
    /// passed through the `on_tool_calls` hook, sealed into `ActiveToolCall`s,
    /// stored in `session.active_tools`, and an `ExecuteTools` effect is
    /// emitted after pushing state to `ExecutingTools`.
    ///
    /// # Complexity
    /// O(p + t) where p = plugins on `route_on_stream_event`, t = pending
    /// tool descriptors.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Core-ChunkBudget, I-Core-ActiveToolCall
    fn dispatch_llm_stream(
        &mut self,
        session: &mut Session,
        event: &StreamEvent,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        if !matches!(session.state, AgentState::Predicting { .. }) {
            return Ok(());
        }

        // Evaluate stream event hooks.
        let route = self.router.routing_table.route_on_stream_event.clone();
        let mut stream_effects = Vec::new();
        let faults = self.eval_route(
            session,
            "on_stream_event",
            &route,
            |plugin, session| {
                plugin
                    .as_on_stream_event()
                    .ok_or(PluginError::Fatal {
                        plugin_name: "<capability_missing>".into(),
                        message: "plugin missing OnStreamEvent capability".into(),
                    })?
                    .on_stream_event(event, &mut session.extensions)
            },
            |action| match action {
                StreamAction::Pass => {}
                StreamAction::Hold => {
                    // Buffering is handled by the plugin / shell.
                }
                StreamAction::OffloadTask { task_id, payload } => {
                    stream_effects.push(Effect::ExecuteCpuTask {
                        task_id: TaskId(task_id),
                        payload,
                    });
                }
            },
        );
        let on_error_effects = self.eval_on_error(session, &faults);
        effects.extend(on_error_effects);
        self.validate_hook_effects(
            crate::engine::hooks::HOOK_INDEX_ON_STREAM_EVENT,
            "on_stream_event",
            &mut stream_effects,
        );
        effects.extend(stream_effects);
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }

        // Mechanical accumulation of assistant text and tool calls.
        self.accumulate_stream_event(session, event);

        // Terminal stream events: finalize prediction and transition state.
        match event {
            StreamEvent::ToolCallDone { .. } => {
                self.finalize_prediction_with_tools(session, effects)?;
            }
            StreamEvent::Done => {
                self.finalize_prediction_text_only(session, effects)?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Dispatch `ToolCallsResult` input.
    ///
    /// Pops the `ExecutingTools` state, runs the `on_tool_result` hook
    /// route, serializes outcomes into history, and transitions back
    /// to `Predicting`.
    ///
    /// # Complexity
    /// O(p + r) where p = plugins on `route_on_tool_result`,
    /// r = number of tool results.
    fn dispatch_tool_calls_result(
        &mut self,
        session: &mut Session,
        generation_id: u64,
        results: &[ToolResultDTO],
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        session.pop_state()?;
        session.active_tools.clear();

        // on_tool_result hook: in-place mutation.
        let mut mutable_results = results.to_vec();
        let route = self.router.routing_table.route_on_tool_result.clone();
        let faults = self.eval_route(
            session,
            "on_tool_result",
            &route,
            |plugin, session| {
                plugin
                    .as_on_tool_result()
                    .ok_or(PluginError::Fatal {
                        plugin_name: "<capability_missing>".into(),
                        message: "plugin missing OnToolResult capability".into(),
                    })?
                    .on_tool_result(&mut mutable_results, &mut session.extensions)
            },
            |_ok| {},
        );
        let on_error_effects = self.eval_on_error(session, &faults);
        effects.extend(on_error_effects);
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }

        // Push results into history.
        for result in &mutable_results {
            session.history.push(ChatMessage::ToolResult {
                id: result.tool_id.clone(),
                content: crate::tool_outcome_to_string(&result.outcome),
            });
        }

        session.push_state(AgentState::Predicting { generation_id })?;

        self.append_state_effects(session, effects);

        Ok(())
    }

    /// Dispatch `RestoreSubRoutine` input.
    ///
    /// If a `SubRoutineHydrator` is configured, the `head_blob` is decoded
    /// and the resulting `Session` is inserted into the registry. On decode
    /// failure the engine falls back to a blank child session and emits an
    /// error effect so the shell can observe the inconsistency.
    ///
    /// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
    fn dispatch_restore_subroutine(
        &mut self,
        _session: &mut Session,
        handle: &SubRoutineHandle,
        head_blob: &[u8],
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        let child = match self.governance.subroutine_hydrator.as_ref() {
            Some(hydrator) => match hydrator.hydrate(head_blob) {
                Ok(session) => session,
                Err(err) => {
                    effects.push(Effect::Error {
                        code: ErrorCode::StateInconsistency,
                        detail: ErrorDetail::TransitionFailed {
                            reason: format!(
                                "sub-routine head deserialization failed for {}: {err}",
                                handle.as_str()
                            ),
                        },
                    });
                    Session::new(handle.as_str())
                }
            },
            None => Session::new(handle.as_str()),
        };

        self.routines.registry.insert(handle.clone(), child);

        effects.push(Effect::SubRoutineRestored {
            handle: handle.clone(),
        });
        effects.push(Effect::SaveSession);

        Ok(())
    }

    /// Mechanically accumulate assistant text and tool call fragments.
    ///
    /// This is pure mechanism: it mutates `Session` state but performs
    /// no policy decisions.
    ///
    /// Refs: I-Core-ChunkBudget, I-Core-StreamNoBranch
    fn accumulate_stream_event(&mut self, session: &mut Session, event: &StreamEvent) {
        match event {
            StreamEvent::TextChunk { chunk, .. } => {
                session
                    .pending_assistant_text
                    .push_str(&String::from_utf8_lossy(chunk));
            }
            StreamEvent::ToolCallStart { id, name, .. } => {
                super::trace::ToolCallAccumulator::on_start(
                    &mut session.extensions,
                    id.clone(),
                    name.clone(),
                );
            }
            StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
                super::trace::ToolCallAccumulator::on_argument_chunk(
                    &mut session.extensions,
                    id,
                    chunk,
                );
            }
            StreamEvent::ToolCallDone { .. } | StreamEvent::Done | StreamEvent::Pass => {
                // Terminal events carry no mechanical accumulation.
            }
        }
    }

    /// Finalize a prediction that produced tool calls.
    ///
    /// Persists assistant text, runs `after_prediction` hooks, drains
    /// accumulated tool descriptors, validates them, and transitions to
    /// `ExecutingTools`.
    ///
    /// Refs: I-Core-ActiveToolCall, I-Core-PluginOrder, I-Core-StreamNoBranch
    fn finalize_prediction_with_tools(
        &mut self,
        session: &mut Session,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        session.persist_assistant_text();
        self.eval_after_prediction(session, effects);

        let pending = super::trace::ToolCallAccumulator::drain(&mut session.extensions);
        if pending.is_empty() {
            return Ok(());
        }

        let mut descriptors = pending;
        self.handle_tool_calls(session, &mut descriptors, effects)?;
        let (active, err_effect) = self.materialize_tool_calls(descriptors);
        if let Some(err) = err_effect {
            effects.push(err);
        }
        session.active_tools = active.clone();

        let generation_id = match session.state.generation_id() {
            Some(id) => id,
            None => {
                // Invariant: ToolCallDone is only valid in Predicting state.
                effects.push(Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    detail: ErrorDetail::StateInconsistent {
                        source: InconsistencySource::Kernel {
                            module: "dispatch::finalize_prediction_with_tools".to_string(),
                        },
                    },
                });
                return Ok(());
            }
        };

        session.push_state(AgentState::ExecutingTools { generation_id })?;
        self.append_state_effects(session, effects);

        Ok(())
    }

    /// Finalize a prediction that produced text only (no tool calls).
    ///
    /// Persists assistant text, runs `after_prediction` hooks, and pops
    /// the `Predicting` state back to the previous state.
    ///
    /// Refs: I-Core-StreamNoBranch
    fn finalize_prediction_text_only(
        &mut self,
        session: &mut Session,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        session.persist_assistant_text();
        self.eval_after_prediction(session, effects);

        if matches!(session.state, AgentState::Predicting { .. }) {
            session.pop_state()?;
            self.append_state_effects(session, effects);
        }

        Ok(())
    }
}

#[cfg(test)]
mod subroutine_hydrator_tests {
    use crate::{
        BriocheEngine, BriocheEngineBuilder, BriocheError, CoreTypes, DecisionAggregator, Effect,
        EngineInput, ErrorCode, ErrorDetail, PluginResult, PolicyDecision, Session,
        SubRoutineHandle, SubRoutineHydrator, SubRoutineLifecycleGuard,
    };

    struct MockDecisionAggregator;

    impl DecisionAggregator<CoreTypes> for MockDecisionAggregator {
        fn aggregate_decisions(
            &self,
            _decisions: Vec<PolicyDecision>,
            _ext: &mut crate::ExtensionStorage,
        ) -> PluginResult<PolicyDecision> {
            Ok(PolicyDecision::Allow)
        }
    }

    struct MockSubRoutineLifecycleGuard;

    impl SubRoutineLifecycleGuard<CoreTypes> for MockSubRoutineLifecycleGuard {
        fn on_exit(
            &self,
            _handle: crate::SubRoutineHandle,
            _parent: &mut Session,
            _registry: &mut crate::SessionRegistry,
        ) -> PluginResult<Vec<Effect>> {
            Ok(Vec::new())
        }
    }

    struct FixedIdHydrator {
        id: String,
        fail: bool,
    }

    impl SubRoutineHydrator<CoreTypes> for FixedIdHydrator {
        fn hydrate(&self, _head_blob: &[u8]) -> Result<Session, BriocheError> {
            if self.fail {
                Err(BriocheError::Serialization("bad blob".to_string()))
            } else {
                Ok(Session::new(self.id.clone()))
            }
        }
    }

    fn take_registered_child(engine: &mut BriocheEngine, handle: &SubRoutineHandle) -> Session {
        match engine.routines.registry.remove(handle) {
            Some(session) => session,
            None => {
                assert_eq!(1, 0, "child session should be registered");
                Session::new("")
            }
        }
    }

    #[test]
    fn hydrator_is_invoked_and_result_used() {
        let mut engine = BriocheEngineBuilder::new()
            .with_decision_aggregator(Box::new(MockDecisionAggregator))
            .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
            .with_subroutine_hydrator(Box::new(FixedIdHydrator {
                id: "hydrated-child".to_string(),
                fail: false,
            }))
            .build();

        let mut parent = Session::new("parent");
        let handle = SubRoutineHandle::new("child-handle");
        let effects = engine.transition(
            &mut parent,
            &EngineInput::RestoreSubRoutine {
                handle: handle.clone(),
                head_blob: vec![0x1, 0x2, 0x3],
            },
        );

        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::SubRoutineRestored { handle: h } if *h == handle
        )));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));

        let child = take_registered_child(&mut engine, &handle);
        assert_eq!(child.id, "hydrated-child");
    }

    #[test]
    fn no_hydrator_falls_back_to_blank_session() {
        let mut engine = BriocheEngineBuilder::new()
            .with_decision_aggregator(Box::new(MockDecisionAggregator))
            .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
            .build();

        let mut parent = Session::new("parent");
        let handle = SubRoutineHandle::new("blank-handle");
        let effects = engine.transition(
            &mut parent,
            &EngineInput::RestoreSubRoutine {
                handle: handle.clone(),
                head_blob: Vec::new(),
            },
        );

        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::SubRoutineRestored { handle: h } if *h == handle
        )));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));
        assert!(!effects.iter().any(|e| matches!(e, Effect::Error { .. })));

        let child = take_registered_child(&mut engine, &handle);
        assert_eq!(child.id, handle.as_str());
    }

    #[test]
    fn hydrator_failure_emits_error_and_fallback() {
        let mut engine = BriocheEngineBuilder::new()
            .with_decision_aggregator(Box::new(MockDecisionAggregator))
            .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
            .with_subroutine_hydrator(Box::new(FixedIdHydrator {
                id: "ignored".to_string(),
                fail: true,
            }))
            .build();

        let mut parent = Session::new("parent");
        let handle = SubRoutineHandle::new("failing-handle");
        let effects = engine.transition(
            &mut parent,
            &EngineInput::RestoreSubRoutine {
                handle: handle.clone(),
                head_blob: vec![0xff],
            },
        );

        assert!(
            matches!(
                effects.first(),
                Some(Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    detail: ErrorDetail::TransitionFailed { reason },
                }) if reason.contains("bad blob")
            ),
            "expected TransitionFailed error, got {effects:?}"
        );
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::SubRoutineRestored { handle: h } if *h == handle
        )));
        assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession)));

        let child = take_registered_child(&mut engine, &handle);
        assert_eq!(child.id, handle.as_str());
    }
}
