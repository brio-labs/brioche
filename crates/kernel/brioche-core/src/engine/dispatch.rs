use crate::{
    AgentState, BriocheError, ChatMessage, Effect, EngineInput, ErrorCode, ErrorDetail,
    PolicyDecision, Session, StreamAction, StreamEvent, SubRoutineHandle, ToolResultDTO,
};

use super::BriocheEngine;

impl BriocheEngine {
    /// Main dispatch — routes `EngineInput` to the appropriate handler.
    ///
    /// Appends effects into the provided buffer rather than returning a new
    /// `Vec`, eliminating per-transition allocations.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Core-RetVecEffect
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
        let mut decisions = Vec::new();
        let route = self.router.routing_table.route_before_prediction.clone();
        let faults = self.eval_route(
            session,
            &route,
            |plugin, session| plugin.before_prediction(&session.history, &mut session.extensions),
            |decision| decisions.push(decision),
        );
        for (name, err) in faults {
            effects.push(Self::plugin_fault(name, err));
        }

        // DecisionAggregator (mandatory if present).
        if let Some(ref aggregator) = self.governance.decision_aggregator {
            match aggregator.aggregate_decisions(decisions, &mut session.extensions) {
                Ok(decision) => match decision {
                    PolicyDecision::Allow => {}
                    PolicyDecision::Block { reason } => {
                        effects.push(Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            detail: ErrorDetail::Generic(reason),
                        });
                        effects.push(Effect::SystemIdle);
                        return Ok(());
                    }
                    PolicyDecision::MutateHistory(edits) => {
                        session.apply_history_edits(&edits)?;
                    }
                    PolicyDecision::RequestEffect(eff) => {
                        effects.push(eff);
                    }
                    PolicyDecision::OverrideTransition(ov) => {
                        effects.extend(ov);
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
        let faults = self.eval_route(
            session,
            &route,
            |plugin, session| plugin.on_stream_event(event, &mut session.extensions),
            |action| match action {
                StreamAction::Pass => {}
                StreamAction::Hold => {
                    // Buffering is handled by the plugin / shell.
                }
                StreamAction::OffloadTask { task_id, payload } => {
                    effects.push(Effect::ExecuteCpuTask { task_id, payload });
                }
            },
        );
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
    ///
    /// Refs: I-Core-PluginOrder, I-Core-ActiveToolCall
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
            &route,
            |plugin, session| plugin.on_tool_result(&mut mutable_results, &mut session.extensions),
            |_ok| {},
        );
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
    /// Sprint 4 placeholder: creates a default child session.
    /// Full `SessionHeadDTO` deserialization deferred to Sprint 5+.
    ///
    /// Refs: I-Shell-Session-NoSend
    fn dispatch_restore_subroutine(
        &mut self,
        _session: &mut Session,
        handle: &SubRoutineHandle,
        _head_blob: &[u8],
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        let child = Session::new(handle.as_str());
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
                        source: "ToolCallDone without Predicting state".into(),
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
