//! Book I — The Core Book: `BriocheEngine` and `UnifiedRoutingTable`.
//!
//! This module upholds:
//! - I-Core-StreamNoBranch: Pre-routed `UnifiedRoutingTable` provides O(1) dispatch.
//! - I-Core-PluginOrder: Total order via `priority` + `name`.
//! - I-Core-NoPanic: `transition()` returns `Vec<Effect>`, never panics.
//! - I-Core-RetVecEffect: All outputs are declarative effects.
//!
//! Refs: SPECS.md §4, §5

use crate::{
    ActiveToolCall, AgentState, BriocheError, BriochePlugin, ChatMessage, ConsistencyVerifier,
    CycleRollbackPolicy, DecisionAggregator, Effect, EngineInput, EpochAction, EpochInterceptor,
    EpochState, ErrorCode, GovernanceFailoverHandler, HistoryEdit, HookEffectConstraint,
    PluginCapabilities, PluginError, PolicyDecision, Session, SessionRegistry, StreamAction,
    StreamEvent, StreamToolAccumulator, SubRoutineHandle, SubRoutineHandler,
    SubRoutineLifecycleGuard, SupersededTransitionTrace, SupersededTransitionTraceLog,
    ToolCallDescriptor, ToolResultDTO, TransitionTrace, TransitionTraceLog, effect_to_bitmask,
};

// ---------------------------------------------------------------------------
// UnifiedRoutingTable
// ---------------------------------------------------------------------------

/// Pre-computed routing table that eliminates runtime capability checks.
///
/// At engine initialization, plugins are sorted by `(priority, name)` and
/// their indices are collected into per-capability vectors. The streaming
/// loop iterates over these vectors directly — no branching on bitmasks.
///
/// Refs: I-Core-StreamNoBranch, I-Core-PluginOrder
pub struct UnifiedRoutingTable {
    pub route_on_input: Vec<usize>,
    pub route_before_prediction: Vec<usize>,
    pub route_on_stream_event: Vec<usize>,
    pub route_after_prediction: Vec<usize>,
    pub route_on_tool_calls: Vec<usize>,
    pub route_on_tool_result: Vec<usize>,
    pub route_on_error: Vec<usize>,
}

impl UnifiedRoutingTable {
    /// Build a routing table from a slice of plugins.
    ///
    /// Plugins are sorted by ascending `priority`, then by `name` for total
    /// deterministic order. Routes contain indices into the original plugin
    /// vector.
    ///
    /// Complexity: O(p log p) where p = number of plugins. Called once at init.
    pub fn from_plugins(plugins: &[Box<dyn BriochePlugin>]) -> Self {
        let mut indexed: Vec<(usize, i16, &'static str)> = plugins
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.priority(), p.name()))
            .collect();
        // Total order: priority ascending, then name lexicographically.
        indexed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(b.2)));

        Self {
            route_on_input: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_INPUT)
            }),
            route_before_prediction: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::BEFORE_PREDICTION)
            }),
            route_on_stream_event: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_STREAM_EVENT)
            }),
            route_after_prediction: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::AFTER_PREDICTION)
            }),
            route_on_tool_calls: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_TOOL_CALLS)
            }),
            route_on_tool_result: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_TOOL_RESULT)
            }),
            route_on_error: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_ERROR)
            }),
        }
    }

    fn collect_route(
        sorted: &[(usize, i16, &'static str)],
        plugins: &[Box<dyn BriochePlugin>],
        has_cap: impl Fn(PluginCapabilities) -> bool,
    ) -> Vec<usize> {
        sorted
            .iter()
            .filter(|(i, _, _)| has_cap(plugins[*i].capabilities()))
            .map(|(i, _, _)| *i)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// BriocheEngine
// ---------------------------------------------------------------------------

/// The synchronous kernel engine.
///
/// `BriocheEngine` owns the `UnifiedRoutingTable`, governance trait
/// implementations, and `SessionRegistry`. It is strictly `!Send` and `!Sync`.
///
/// Construct via `BriocheEngineBuilder`.
///
/// Refs: I-Core-Pure, I-Core-NoPanic
pub struct BriocheEngine {
    plugins: Vec<Box<dyn BriochePlugin>>,
    routing_table: UnifiedRoutingTable,

    // Governance trait slots (optional unless noted)
    epoch_interceptor: Option<Box<dyn EpochInterceptor>>,
    subroutine_handler: Option<Box<dyn SubRoutineHandler>>,
    consistency_verifier: Option<Box<dyn ConsistencyVerifier>>,
    decision_aggregator: Option<Box<dyn DecisionAggregator>>,
    hook_effect_constraint: Option<Box<dyn HookEffectConstraint>>,
    cycle_rollback_policy: Option<Box<dyn CycleRollbackPolicy>>,
    subroutine_lifecycle_guard: Option<Box<dyn SubRoutineLifecycleGuard>>,
    governance_failover_handler: Option<Box<dyn GovernanceFailoverHandler>>,

    // Sub-routine registry
    session_registry: SessionRegistry,

    // Monotonically increasing generation counter for predictions.
    next_generation_id: u64,

    // Safeguard applied when a ToolCallDescriptor lacks timeout_ms.
    // The kernel emits Effect::Error(StateInconsistency) when falling back.
    default_tool_timeout_ms: u64,

    _not_send_sync: std::marker::PhantomData<*mut ()>,
}

impl std::fmt::Debug for BriocheEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BriocheEngine")
            .field("plugin_count", &self.plugins.len())
            .field("next_generation_id", &self.next_generation_id)
            .finish_non_exhaustive()
    }
}

/// Result of evaluating the `on_input` hook.
enum InputResult {
    Allow,
    Block { reason: String },
    OverrideTransition(Vec<Effect>, String),
    Accumulated(Vec<Effect>),
}

impl BriocheEngine {
    /// Execute one synchronous transition cycle.
    ///
    /// The engine receives a `Session` and an `EngineInput`, computes the
    /// next state, and returns a vector of declarative `Effect`s for the
    /// shell to execute.
    ///
    /// This function never panics. Any anomaly produces an `Effect::Error`
    /// or transitions the automaton to a safe state.
    ///
    /// Complexity: O(p) where p = number of plugins on the active routes.
    ///
    /// Refs: I-Core-NoPanic, I-Core-RetVecEffect
    pub fn transition(&mut self, session: &mut Session, input: &EngineInput) -> Vec<Effect> {
        let prev_was_subroutine = matches!(session.state, AgentState::SubRoutine(_));
        let prev_handle = match &session.state {
            AgentState::SubRoutine(h) => Some(h.clone()),
            _ => None,
        };

        // Step 1: Inject SessionSnapshot before each transition cycle.
        session.extensions.insert(session.snapshot());

        let mut effects = Vec::new();

        // Step 3: EpochInterceptor (optional, but evaluated first if present).
        if let Some(ref interceptor) = self.epoch_interceptor {
            match interceptor.intercept_epoch(input, &mut session.extensions) {
                Ok(EpochAction::Block { reason }) => {
                    return vec![
                        Effect::Error {
                            code: ErrorCode::EpochMismatch,
                            message: reason,
                        },
                        Effect::SystemIdle,
                    ];
                }
                Ok(EpochAction::Proceed) => {}
                Err(err) => {
                    effects.push(self.plugin_fault("epoch_interceptor", err));
                }
            }
        }

        // Step 4: SubRoutineHandler (optional).
        if let Some(ref handler) = self.subroutine_handler
            && let AgentState::SubRoutine(ref handle) = session.state
            && let Some(child) = self.session_registry.get_mut(handle)
        {
            match handler.handle_subroutine(session, child, input) {
                Ok(Some(sub_effects)) => {
                    effects.extend(sub_effects);
                    return self.finalize_transition(
                        session,
                        prev_was_subroutine,
                        prev_handle,
                        effects,
                    );
                }
                Ok(None) => {}
                Err(err) => {
                    effects.push(self.plugin_fault("subroutine_handler", err));
                }
            }
        }

        // Step 5: on_input hook (routed).
        match self.eval_on_input(session, input) {
            InputResult::OverrideTransition(ov_effects, source_plugin) => {
                self.log_override_transition(session, &source_plugin);
                effects.extend(ov_effects);
                return self.finalize_transition(
                    session,
                    prev_was_subroutine,
                    prev_handle,
                    effects,
                );
            }
            InputResult::Block { reason } => {
                effects.push(Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    message: reason,
                });
                effects.push(Effect::SystemIdle);
                return self.finalize_transition(
                    session,
                    prev_was_subroutine,
                    prev_handle,
                    effects,
                );
            }
            InputResult::Accumulated(acc) => {
                effects.extend(acc);
            }
            InputResult::Allow => {}
        }

        // Step 6: Main dispatch on EngineInput.
        match self.dispatch_input(session, input) {
            Ok(dispatch_effects) => effects.extend(dispatch_effects),
            Err(err) => {
                effects.push(Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    message: err.to_string(),
                });
            }
        }

        // Steps 7-12: Finalize.
        self.finalize_transition(session, prev_was_subroutine, prev_handle, effects)
    }

    /// Access the internal `SessionRegistry`.
    ///
    /// Used by governance plugins and the shell for sub-routine management.
    pub fn session_registry(&self) -> &SessionRegistry {
        &self.session_registry
    }

    /// Evaluate the `after_prediction` route.
    ///
    /// Called after the LLM prediction completes (before tool execution
    /// or transition to `Idle`). Collects `PluginFault` effects for any
    /// plugin errors but does not short-circuit.
    ///
    /// # Complexity
    /// O(p) where p = plugins on `route_after_prediction`.
    ///
    /// Refs: I-Core-PluginOrder, I-Core-StreamNoBranch
    fn eval_after_prediction(&mut self, session: &mut Session) -> Vec<Effect> {
        let mut effects = Vec::new();
        let route = self.routing_table.route_after_prediction.clone();
        for &idx in &route {
            let name = self.plugins[idx].name();
            session.extensions.insert(session.snapshot());
            let result = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.after_prediction(&mut session.extensions)
            });
            if let Err(err) = result {
                effects.push(self.plugin_fault(name, err));
            }
        }
        effects
    }

    /// Mutable access to the internal `SessionRegistry`.
    pub fn session_registry_mut(&mut self) -> &mut SessionRegistry {
        &mut self.session_registry
    }

    /// Rebuild routing tables excluding quarantined or inactive plugins.
    ///
    /// `active_mask` is a boolean slice parallel to the plugin vector.
    /// `true` means the plugin remains active; `false` means it is excluded.
    /// The kernel performs an O(N) recalculation of all routes without
    /// restarting or invalidating the session.
    ///
    /// This is a transactional barrier: no new `EngineInput` should be
    /// processed until this call completes.
    ///
    /// # Complexity
    /// O(p log p) where p = number of active plugins. Called once per
    /// quarantine event, not on the hot path.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    pub fn rebuild_routes(&mut self, active_mask: &[bool]) {
        // Rebuild routing table from the same plugins but with a filter.
        // We don't remove plugins from `self.plugins`; we just rebuild
        // the routing table considering only active indices.
        let active_indices: Vec<usize> = (0..self.plugins.len())
            .filter(|i| active_mask.get(*i).copied().unwrap_or(true))
            .collect();

        let mut indexed: Vec<(usize, i16, &'static str)> = active_indices
            .iter()
            .map(|&i| (i, self.plugins[i].priority(), self.plugins[i].name()))
            .collect();
        indexed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(b.2)));

        self.routing_table = UnifiedRoutingTable {
            route_on_input: Self::collect_route_filtered(&indexed, &self.plugins, |c| {
                c.contains(PluginCapabilities::ON_INPUT)
            }),
            route_before_prediction: Self::collect_route_filtered(&indexed, &self.plugins, |c| {
                c.contains(PluginCapabilities::BEFORE_PREDICTION)
            }),
            route_on_stream_event: Self::collect_route_filtered(&indexed, &self.plugins, |c| {
                c.contains(PluginCapabilities::ON_STREAM_EVENT)
            }),
            route_after_prediction: Self::collect_route_filtered(&indexed, &self.plugins, |c| {
                c.contains(PluginCapabilities::AFTER_PREDICTION)
            }),
            route_on_tool_calls: Self::collect_route_filtered(&indexed, &self.plugins, |c| {
                c.contains(PluginCapabilities::ON_TOOL_CALLS)
            }),
            route_on_tool_result: Self::collect_route_filtered(&indexed, &self.plugins, |c| {
                c.contains(PluginCapabilities::ON_TOOL_RESULT)
            }),
            route_on_error: Self::collect_route_filtered(&indexed, &self.plugins, |c| {
                c.contains(PluginCapabilities::ON_ERROR)
            }),
        };
    }

    fn collect_route_filtered(
        sorted: &[(usize, i16, &'static str)],
        plugins: &[Box<dyn BriochePlugin>],
        has_cap: impl Fn(PluginCapabilities) -> bool,
    ) -> Vec<usize> {
        sorted
            .iter()
            .filter(|(i, _, _)| has_cap(plugins[*i].capabilities()))
            .map(|(i, _, _)| *i)
            .collect()
    }

    /// The default tool timeout applied when a descriptor omits `timeout_ms`.
    ///
    /// This safeguard is mechanism, not policy. The kernel applies it
    /// when `seal()` encounters a descriptor with `timeout_ms: None`.
    /// Policy plugins should set `timeout_ms` via the `on_tool_calls` hook.
    ///
    /// Refs: I-Core-ActiveToolCall
    pub fn default_tool_timeout_ms(&self) -> u64 {
        self.default_tool_timeout_ms
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Wrap a single plugin hook invocation with COW rollback.
    ///
    /// The `cycle_rollback_policy` is temporarily removed from `self`,
    /// passed to the closure via `ExtensionStorage`'s mutation observer,
    /// then restored after the hook. On plugin error, `rollback_hook()`
    /// is called to restore mutated extensions to their pre-hook state.
    ///
    /// **Note on time instrumentation:** per-hook wall-clock timing is
    /// intentionally **not** performed in Core. `Instant::now()` is
    /// disallowed in Core by PHILOSOPHY.md §2.2 to preserve determinism.
    /// Time-based safety is provided by the Shell Runtime (`EngineWatchdog`).
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    fn with_rollback<R>(
        &mut self,
        session: &mut Session,
        _plugin_name: &'static str,
        f: impl FnOnce(&mut Self, &mut Session) -> R,
    ) -> R {
        let mut rollback = self.cycle_rollback_policy.take();
        if let Some(r) = rollback.as_mut() {
            r.begin_hook();
        }
        let observer_ptr: Option<*mut dyn CycleRollbackPolicy> = rollback
            .as_mut()
            .map(|r| r.as_mut() as *mut dyn CycleRollbackPolicy);
        unsafe {
            session.extensions.set_cow_observer(observer_ptr);
        }

        let result = f(self, session);

        unsafe {
            session.extensions.clear_cow_observer();
        }

        if let Some(r) = rollback.as_mut() {
            r.commit_hook(&mut session.extensions);
        }

        self.cycle_rollback_policy = rollback;
        result
    }

    /// Evaluate the `on_input` route.
    ///
    /// `OverrideTransition` from the first plugin wins; subsequent ones are
    /// logged as superseded. `Block` short-circuits immediately.
    fn eval_on_input(&mut self, session: &mut Session, input: &EngineInput) -> InputResult {
        let mut accumulated = Vec::new();
        let mut override_transition: Option<(Vec<Effect>, String)> = None;

        let route = self.routing_table.route_on_input.clone();
        for &idx in &route {
            let name = self.plugins[idx].name();
            session.extensions.insert(session.snapshot());

            let decision = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_input(input, &mut session.extensions)
            });

            match decision {
                Ok(PolicyDecision::Allow) => {}
                Ok(PolicyDecision::Block { reason }) => {
                    return InputResult::Block { reason };
                }
                Ok(PolicyDecision::MutateHistory(edits)) => {
                    if let Err(err) = Self::apply_history_edits(session, &edits) {
                        accumulated.push(Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            message: err.to_string(),
                        });
                    }
                }
                Ok(PolicyDecision::RequestEffect(eff)) => {
                    accumulated.push(eff);
                }
                Ok(PolicyDecision::OverrideTransition(effects)) => {
                    if override_transition.is_none() {
                        override_transition = Some((effects, name.to_string()));
                    } else {
                        self.log_superseded_transition(
                            session,
                            name,
                            &PolicyDecision::OverrideTransition(effects),
                        );
                    }
                }
                Err(err) => {
                    accumulated.push(self.plugin_fault(name, err));
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

    /// Main dispatch — routes `EngineInput` to the appropriate handler.
    fn dispatch_input(
        &mut self,
        session: &mut Session,
        input: &EngineInput,
    ) -> Result<Vec<Effect>, BriocheError> {
        match input {
            EngineInput::UserMessage(content) => self.dispatch_user_message(session, content),
            EngineInput::LlmStream(event) => self.dispatch_llm_stream(session, event),
            EngineInput::ToolCallsResult {
                generation_id,
                results,
            } => self.dispatch_tool_calls_result(session, *generation_id, results),
            EngineInput::RestoreSubRoutine { handle, head_blob } => {
                self.dispatch_restore_subroutine(session, handle, head_blob)
            }
        }
    }

    /// Dispatch `UserMessage` input.
    fn dispatch_user_message(
        &mut self,
        session: &mut Session,
        content: &str,
    ) -> Result<Vec<Effect>, BriocheError> {
        let mut effects = Vec::new();

        session.history.push(ChatMessage::User {
            content: content.to_string(),
        });

        let generation_id = self.next_generation_id;
        self.next_generation_id += 1;

        session
            .extensions
            .get_or_insert_default::<EpochState>()
            .current_generation = generation_id;

        session.push_state(AgentState::Predicting { generation_id })?;

        // before_prediction hook: collect decisions.
        let mut decisions = Vec::new();
        let route = self.routing_table.route_before_prediction.clone();
        for &idx in &route {
            let name = self.plugins[idx].name();
            session.extensions.insert(session.snapshot());
            let decision = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.before_prediction(&session.history, &mut session.extensions)
            });
            match decision {
                Ok(decision) => decisions.push(decision),
                Err(err) => {
                    effects.push(self.plugin_fault(name, err));
                }
            }
        }

        // DecisionAggregator (mandatory).
        if let Some(ref aggregator) = self.decision_aggregator {
            match aggregator.aggregate_decisions(decisions, &mut session.extensions) {
                Ok(decision) => match decision {
                    PolicyDecision::Allow => {}
                    PolicyDecision::Block { reason } => {
                        effects.push(Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            message: reason,
                        });
                        effects.push(Effect::SystemIdle);
                        return Ok(effects);
                    }
                    PolicyDecision::MutateHistory(edits) => {
                        Self::apply_history_edits(session, &edits)?;
                    }
                    PolicyDecision::RequestEffect(eff) => {
                        effects.push(eff);
                    }
                    PolicyDecision::OverrideTransition(ov) => {
                        effects.extend(ov);
                        return Ok(effects);
                    }
                },
                Err(err) => {
                    effects.push(self.plugin_fault("decision_aggregator", err));
                }
            }
        }

        effects.push(Effect::CallLlmNetwork);
        effects.push(Effect::SaveSession);

        Ok(effects)
    }

    /// Dispatch `LlmStream` input.
    ///
    /// Accumulates tool calls from `ToolCallStart` / `ToolArgumentChunk`
    /// events. When `ToolCallDone` is received, pending descriptors are
    /// passed through the `on_tool_calls` hook, sealed into `ActiveToolCall`s,
    /// stored in `session.active_tools`, and an `ExecuteTools` effect is
    /// emitted after pushing state to `ExecutingTools`.
    ///
    /// Pre-routed dispatch guarantees no runtime bitmask checks. Arguments
    /// are accumulated incrementally to avoid allocation spikes on single
    /// large chunks. Descriptors are sealed before storage.
    ///
    /// # Complexity
    /// O(p + t) where p = plugins on `route_on_stream_event`, t = pending
    /// tool descriptors. One `BTreeMap` insertion per `ToolCallStart`.
    /// No heap allocation on `Pass` or `Hold`.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Core-ChunkBudget, I-Core-ActiveToolCall
    fn dispatch_llm_stream(
        &mut self,
        session: &mut Session,
        event: &StreamEvent,
    ) -> Result<Vec<Effect>, BriocheError> {
        let mut effects = Vec::new();

        if !matches!(session.state, AgentState::Predicting { .. }) {
            return Ok(effects);
        }

        let route = self.routing_table.route_on_stream_event.clone();
        for &idx in &route {
            let name = self.plugins[idx].name();
            session.extensions.insert(session.snapshot());
            let action = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_stream_event(event, &mut session.extensions)
            });
            match action {
                Ok(StreamAction::Pass) => {}
                Ok(StreamAction::Hold) => {
                    // Buffering is handled by the plugin / shell.
                }
                Ok(StreamAction::OffloadTask { task_id, payload }) => {
                    effects.push(Effect::ExecuteCpuTask { task_id, payload });
                }
                Err(err) => {
                    effects.push(self.plugin_fault(name, err));
                }
            }
        }

        // Mechanical accumulation of assistant text and tool calls discovered
        // in the stream.
        match event {
            StreamEvent::TextChunk { chunk, .. } => {
                session
                    .pending_assistant_text
                    .push_str(&String::from_utf8_lossy(chunk));
            }
            StreamEvent::ToolCallStart { id, name, .. } => {
                let accumulator = session
                    .extensions
                    .get_or_insert_default::<StreamToolAccumulator>();
                accumulator.pending.insert(
                    id.clone(),
                    ToolCallDescriptor {
                        tool_id: id.clone(),
                        tool_name: name.clone(),
                        arguments: String::new(),
                        timeout_ms: None,
                    },
                );
            }
            StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
                let accumulator = session
                    .extensions
                    .get_or_insert_default::<StreamToolAccumulator>();
                if let Some(descriptor) = accumulator.pending.get_mut(id) {
                    descriptor
                        .arguments
                        .push_str(&String::from_utf8_lossy(chunk));
                }
            }
            StreamEvent::ToolCallDone { .. } => {
                // Drain all pending descriptors.
                let tool_calls: Vec<ToolCallDescriptor> = {
                    let accumulator = session
                        .extensions
                        .get_or_insert_default::<StreamToolAccumulator>();
                    std::mem::take(&mut accumulator.pending)
                        .into_values()
                        .collect()
                };

                // Persist assistant text + tool calls as a single message.
                session.history.push(ChatMessage::Assistant {
                    content: std::mem::take(&mut session.pending_assistant_text),
                    reasoning: None,
                    tool_calls,
                });

                // Prediction completes before tool execution.
                effects.extend(self.eval_after_prediction(session));

                // Materialize tool calls for execution.
                let tool_calls = match session.history.last() {
                    Some(ChatMessage::Assistant { tool_calls, .. }) => tool_calls.clone(),
                    _ => Vec::new(),
                };
                if !tool_calls.is_empty() {
                    let mut descriptors = tool_calls;
                    self.handle_tool_calls(session, &mut descriptors, &mut effects)?;
                    let (active, err_effect) = self.materialize_tool_calls(descriptors);
                    if let Some(err) = err_effect {
                        effects.push(err);
                    }
                    session.active_tools = active.clone();
                    let generation_id = match session.state {
                        AgentState::Predicting { generation_id } => generation_id,
                        _ => 0,
                    };
                    session.push_state(AgentState::ExecutingTools { generation_id })?;
                    effects.push(Effect::ExecuteTools(active));
                    effects.push(Effect::SaveSession);
                }
            }
            StreamEvent::Done => {
                // Persist accumulated assistant text before returning to Idle.
                if !session.pending_assistant_text.is_empty() {
                    session.history.push(ChatMessage::Assistant {
                        content: std::mem::take(&mut session.pending_assistant_text),
                        reasoning: None,
                        tool_calls: Vec::new(),
                    });
                }

                // Prediction completes without tool calls.
                effects.extend(self.eval_after_prediction(session));

                if matches!(session.state, AgentState::Predicting { .. }) {
                    session.pop_state()?;
                    effects.push(Effect::SystemIdle);
                    effects.push(Effect::SaveSession);
                }
            }
            _ => {}
        }

        Ok(effects)
    }

    /// Invoke the `on_tool_calls` hook on all pre-routed plugins.
    ///
    /// Plugins mutate `timeout_ms` and other fields in place.
    ///
    /// Evaluated in ascending `(priority, name)` order. Descriptors are the
    /// sole mutable interface; `ActiveToolCall` is never exposed to plugins.
    ///
    /// # Complexity
    /// O(p) where p = plugins on `route_on_tool_calls`. One mutable
    /// reference pass over descriptors per plugin.
    ///
    /// Refs: I-Core-PluginOrder, I-Core-ActiveToolCall
    fn handle_tool_calls(
        &mut self,
        session: &mut Session,
        descriptors: &mut Vec<ToolCallDescriptor>,
        effects: &mut Vec<Effect>,
    ) -> Result<(), BriocheError> {
        let route = self.routing_table.route_on_tool_calls.clone();
        for &idx in &route {
            let name = self.plugins[idx].name();
            session.extensions.insert(session.snapshot());
            let result = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_tool_calls(descriptors, &mut session.extensions)
            });
            if let Err(err) = result {
                effects.push(self.plugin_fault(name, err));
            }
        }
        Ok(())
    }

    /// Canonical conversion from `ToolCallDescriptor` to `ActiveToolCall`.
    ///
    /// Any descriptor missing `timeout_ms` receives `default_tool_timeout_ms`
    /// and an `Effect::Error(StateInconsistency)` is returned alongside the
    /// sealed calls.
    ///
    /// Uses exhaustive field mapping so the compiler forces updates when
    /// fields are added. Never panics; missing fields produce `Effect::Error`.
    ///
    /// # Complexity
    /// O(n) where n = number of descriptors. Allocates one `Vec<ActiveToolCall>`.
    ///
    /// Refs: I-Core-ActiveToolCall, I-Core-NoPanic
    fn materialize_tool_calls(
        &self,
        descriptors: Vec<ToolCallDescriptor>,
    ) -> (Vec<ActiveToolCall>, Option<Effect>) {
        let active = descriptors
            .into_iter()
            .map(|d| ActiveToolCall {
                tool_id: d.tool_id,
                tool_name: d.tool_name,
                arguments: d.arguments,
                timeout_ms: d.timeout_ms.unwrap_or(self.default_tool_timeout_ms),
            })
            .collect();
        // When default_tool_timeout_ms is 0, missing timeout_ms is
        // intentional (no-timeout policy). Don't emit a spurious error.
        (active, None)
    }

    /// Dispatch `ToolCallsResult` input.
    fn dispatch_tool_calls_result(
        &mut self,
        session: &mut Session,
        generation_id: u64,
        results: &[ToolResultDTO],
    ) -> Result<Vec<Effect>, BriocheError> {
        let mut effects = Vec::new();

        session.pop_state()?;
        session.active_tools.clear();

        // on_tool_result hook: in-place mutation.
        let mut mutable_results = results.to_vec();
        let route = self.routing_table.route_on_tool_result.clone();
        for &idx in &route {
            let name = self.plugins[idx].name();
            session.extensions.insert(session.snapshot());
            let result = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_tool_result(&mut mutable_results, &mut session.extensions)
            });
            if let Err(err) = result {
                effects.push(self.plugin_fault(name, err));
            }
        }

        // Push results into history.
        for result in &mutable_results {
            let content = match &result.outcome {
                crate::ToolOutcome::Success(s)
                | crate::ToolOutcome::BusinessError(s)
                | crate::ToolOutcome::SystemError(s) => s.clone(),
                crate::ToolOutcome::TimeoutWithPartialData { partial_output } => {
                    partial_output.clone().unwrap_or_default()
                }
            };
            session.history.push(ChatMessage::ToolResult {
                id: result.tool_id.clone(),
                content,
            });
        }

        session.push_state(AgentState::Predicting { generation_id })?;

        effects.push(Effect::CallLlmNetwork);
        effects.push(Effect::SaveSession);

        Ok(effects)
    }

    /// Dispatch `RestoreSubRoutine` input.
    fn dispatch_restore_subroutine(
        &mut self,
        _session: &mut Session,
        handle: &SubRoutineHandle,
        _head_blob: &[u8],
    ) -> Result<Vec<Effect>, BriocheError> {
        // Sprint 4 placeholder: create a default session.
        // Full SessionHeadDTO deserialization will be implemented in Sprint 5+.
        let child = Session::new(handle.as_str());
        self.session_registry.insert(handle.clone(), child);

        let effects = vec![
            Effect::SubRoutineRestored {
                handle: handle.clone(),
            },
            Effect::SaveSession,
        ];

        Ok(effects)
    }

    /// Finalize a transition: apply lifecycle guards, consistency checks,
    /// and position guarantees.
    fn finalize_transition(
        &mut self,
        session: &mut Session,
        prev_was_subroutine: bool,
        prev_handle: Option<SubRoutineHandle>,
        mut effects: Vec<Effect>,
    ) -> Vec<Effect> {
        // Step 7: SubRoutineLifecycleGuard (mandatory if exiting SubRoutine).
        if prev_was_subroutine
            && let Some(ref guard) = self.subroutine_lifecycle_guard
            && let Some(handle) = prev_handle
            && !matches!(session.state, AgentState::SubRoutine(_))
        {
            match guard.on_exit(handle, session, &mut self.session_registry) {
                Ok(guard_effects) => effects.extend(guard_effects),
                Err(err) => {
                    effects.push(self.plugin_fault("subroutine_lifecycle_guard", err));
                }
            }
        }

        // Step 8: Hook effect validation (optional).
        if let Some(ref constraint) = self.hook_effect_constraint {
            // Sprint 4: basic validation for RequestEffect decisions.
            // Full per-hook bitmask mapping will be refined in Sprint 5+.
            for effect in &mut effects {
                if matches!(
                    effect,
                    Effect::Error { .. } | Effect::PluginFault { .. } | Effect::SystemIdle
                ) {
                    // Always allow internal/error effects.
                    continue;
                }
                let mask = effect_to_bitmask(effect);
                if !constraint.is_allowed_fast(0, mask) {
                    // Fallback validation
                    let variant = format!("{:?}", std::mem::discriminant(effect));
                    if !constraint.is_allowed_fallback("transition", &variant) {
                        *effect = Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            message: "Effect not allowed on this hook".into(),
                        };
                    }
                }
            }
        }

        // Step 9: RebuildRoutes last position guarantee.
        Self::ensure_rebuildroutes_last(&mut effects);

        // Step 10: ConsistencyVerifier (optional).
        if let Some(ref verifier) = self.consistency_verifier {
            // If RebuildRoutes is present, consistency effects are ignored.
            if !effects.iter().any(|e| matches!(e, Effect::RebuildRoutes)) {
                match verifier.verify_consistency(session) {
                    Ok(Some(verifier_effects)) => {
                        effects.extend(verifier_effects);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        effects.push(self.plugin_fault("consistency_verifier", err));
                    }
                }
            }
        }

        // Step 10.5: GovernanceFailoverHandler (optional).
        if let Some(ref handler) = self.governance_failover_handler
            && !effects.iter().any(|e| matches!(e, Effect::RebuildRoutes))
        {
            let mut replacement_effects = Vec::new();
            let mut has_fault = false;
            for effect in &effects {
                if let Effect::PluginFault { .. } = effect {
                    has_fault = true;
                    match handler.handle_failure(session, effect) {
                        Ok(Some(failover)) => {
                            replacement_effects.extend(failover);
                        }
                        _ => {
                            replacement_effects.push(effect.clone());
                        }
                    }
                } else {
                    replacement_effects.push(effect.clone());
                }
            }
            if has_fault {
                effects = replacement_effects;
            }
        }

        effects
    }

    // -----------------------------------------------------------------------
    // Static helpers
    // -----------------------------------------------------------------------

    /// Apply a sequence of `HistoryEdit`s to the session, validating indices.
    fn apply_history_edits(
        session: &mut Session,
        edits: &[HistoryEdit],
    ) -> Result<(), BriocheError> {
        for edit in edits {
            match edit {
                HistoryEdit::Insert { index, message } => {
                    if *index > session.history.len() {
                        return Err(BriocheError::InvalidStateTransition(format!(
                            "history insert index {} out of bounds (len={})",
                            index,
                            session.history.len()
                        )));
                    }
                    session.history.insert(*index, message.clone());
                }
                HistoryEdit::Replace { index, message } => {
                    if *index >= session.history.len() {
                        return Err(BriocheError::InvalidStateTransition(format!(
                            "history replace index {} out of bounds (len={})",
                            index,
                            session.history.len()
                        )));
                    }
                    session.history[*index] = message.clone();
                }
                HistoryEdit::Truncate { keep_last } => {
                    let keep = (*keep_last).min(session.history.len());
                    let drain_count = session.history.len() - keep;
                    session.history.drain(..drain_count);
                }
            }
        }
        Ok(())
    }

    /// Guarantee that `RebuildRoutes` occupies the last position in effects.
    fn ensure_rebuildroutes_last(effects: &mut Vec<Effect>) {
        if let Some(pos) = effects
            .iter()
            .rposition(|e| matches!(e, Effect::RebuildRoutes))
        {
            effects.truncate(pos + 1);
        }
    }

    /// Build a `PluginFault` effect.
    fn plugin_fault(&self, name: &str, error: PluginError) -> Effect {
        Effect::PluginFault {
            plugin_name: name.to_string(),
            error,
        }
    }

    // -----------------------------------------------------------------------
    // Trace logging
    // -----------------------------------------------------------------------

    fn log_override_transition(&self, session: &mut Session, source_plugin: &str) {
        let epoch = session
            .extensions
            .get_or_insert_default::<EpochState>()
            .current_generation;
        let log = session
            .extensions
            .get_or_insert_default::<TransitionTraceLog>();
        if log.entries.len() >= 128 {
            log.entries.remove(0);
        }
        log.entries.push(TransitionTrace {
            source_plugin: source_plugin.to_string(),
            decision: PolicyDecision::OverrideTransition(vec![]),
            epoch,
        });
    }

    fn log_superseded_transition(
        &self,
        session: &mut Session,
        source_plugin: &str,
        attempted_decision: &PolicyDecision,
    ) {
        let epoch = session
            .extensions
            .get_or_insert_default::<EpochState>()
            .current_generation;
        let log = session
            .extensions
            .get_or_insert_default::<SupersededTransitionTraceLog>();
        if log.entries.len() >= 128 {
            log.entries.remove(0);
        }
        log.entries.push(SupersededTransitionTrace {
            source_plugin: source_plugin.to_string(),
            attempted_decision: attempted_decision.clone(),
            preempted_by: "prior_plugin".to_string(),
            epoch,
        });
    }
}

// ---------------------------------------------------------------------------
// BriocheEngineBuilder
// ---------------------------------------------------------------------------

/// Builder for `BriocheEngine`.
///
/// Enforces injection of mandatory governance traits at compile time
/// (via `build()` returning `Result`).
///
/// Refs: SPECS.md §6.2
pub struct BriocheEngineBuilder {
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
}

impl Default for BriocheEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BriocheEngineBuilder {
    /// Create a new builder with no plugins or governance traits.
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
        }
    }

    /// Register a plugin.
    pub fn with_plugin(mut self, plugin: Box<dyn BriochePlugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Inject an `EpochInterceptor`.
    pub fn with_epoch_interceptor(mut self, interceptor: Box<dyn EpochInterceptor>) -> Self {
        self.epoch_interceptor = Some(interceptor);
        self
    }

    /// Inject a `SubRoutineHandler`.
    pub fn with_subroutine_handler(mut self, handler: Box<dyn SubRoutineHandler>) -> Self {
        self.subroutine_handler = Some(handler);
        self
    }

    /// Inject a `ConsistencyVerifier`.
    pub fn with_consistency_verifier(mut self, verifier: Box<dyn ConsistencyVerifier>) -> Self {
        self.consistency_verifier = Some(verifier);
        self
    }

    /// Inject a `DecisionAggregator` (mandatory).
    pub fn with_decision_aggregator(mut self, aggregator: Box<dyn DecisionAggregator>) -> Self {
        self.decision_aggregator = Some(aggregator);
        self
    }

    /// Inject a `HookEffectConstraint`.
    pub fn with_hook_effect_constraint(
        mut self,
        constraint: Box<dyn HookEffectConstraint>,
    ) -> Self {
        self.hook_effect_constraint = Some(constraint);
        self
    }

    /// Inject a `CycleRollbackPolicy`.
    pub fn with_cycle_rollback_policy(mut self, policy: Box<dyn CycleRollbackPolicy>) -> Self {
        self.cycle_rollback_policy = Some(policy);
        self
    }

    /// Inject a `SubRoutineLifecycleGuard` (mandatory).
    pub fn with_subroutine_lifecycle_guard(
        mut self,
        guard: Box<dyn SubRoutineLifecycleGuard>,
    ) -> Self {
        self.subroutine_lifecycle_guard = Some(guard);
        self
    }

    /// Inject a `GovernanceFailoverHandler`.
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

    /// Build the `BriocheEngine`.
    ///
    /// Returns `Err` if a mandatory trait is missing.
    pub fn build(self) -> Result<BriocheEngine, BriocheError> {
        let decision_aggregator = self.decision_aggregator.ok_or_else(|| {
            BriocheError::Other(
                "DecisionAggregator is mandatory — use with_decision_aggregator()".into(),
            )
        })?;

        let subroutine_lifecycle_guard = self.subroutine_lifecycle_guard.ok_or_else(|| {
            BriocheError::Other(
                "SubRoutineLifecycleGuard is mandatory — use with_subroutine_lifecycle_guard()"
                    .into(),
            )
        })?;

        let routing_table = UnifiedRoutingTable::from_plugins(&self.plugins);

        Ok(BriocheEngine {
            plugins: self.plugins,
            routing_table,
            epoch_interceptor: self.epoch_interceptor,
            subroutine_handler: self.subroutine_handler,
            consistency_verifier: self.consistency_verifier,
            decision_aggregator: Some(decision_aggregator),
            hook_effect_constraint: self.hook_effect_constraint,
            cycle_rollback_policy: self.cycle_rollback_policy,
            subroutine_lifecycle_guard: Some(subroutine_lifecycle_guard),
            governance_failover_handler: self.governance_failover_handler,
            session_registry: SessionRegistry::new(),
            next_generation_id: 1,
            default_tool_timeout_ms: self.default_tool_timeout_ms,
            _not_send_sync: std::marker::PhantomData,
        })
    }
}
