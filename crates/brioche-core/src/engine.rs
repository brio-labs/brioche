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
    CowBudgetPolicy, CycleRollbackPolicy, DecisionAggregator, Effect, EngineInput, EpochAction,
    EpochInterceptor, EpochState, ErrorCode, GovernanceFailoverHandler, HistoryEdit,
    HookEffectConstraint, PluginCapabilities, PluginError, PolicyDecision, Session,
    SessionRegistry, SignalDrainOrder, StreamAction, StreamEvent, StreamToolAccumulator,
    SubRoutineHandle, SubRoutineHandler, SubRoutineLifecycleGuard, SupersededTransitionTrace,
    SupersededTransitionTraceLog, TRACE_LOG_CAPACITY, ToolCallDescriptor, ToolResultDTO,
    TransitionTrace, TransitionTraceLog, effect_to_bitmask,
};

// ---------------------------------------------------------------------------
// Helper trait for with_rollback
// ---------------------------------------------------------------------------

/// Trait used by `with_rollback` to decide between `commit_hook` and `rollback_hook`.
trait IsResultErr {
    fn is_err(&self) -> bool;
}

impl<T, E> IsResultErr for Result<T, E> {
    fn is_err(&self) -> bool {
        Result::is_err(self)
    }
}

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
    // Mandatory: validated at build time and therefore non-optional.
    decision_aggregator: Box<dyn DecisionAggregator>,
    hook_effect_constraint: Option<Box<dyn HookEffectConstraint>>,
    cycle_rollback_policy: Option<Box<dyn CycleRollbackPolicy>>,
    // Mandatory: validated at build time and therefore non-optional.
    subroutine_lifecycle_guard: Box<dyn SubRoutineLifecycleGuard>,
    governance_failover_handler: Option<Box<dyn GovernanceFailoverHandler>>,
    // Optional drain adapter for separate signal channels.
    signal_drain_order: Option<Box<dyn SignalDrainOrder>>,
    // Optional per-hook COW budget policy (reserved for Sprint 5+).
    #[allow(dead_code)]
    cow_budget_policy: Option<Box<dyn CowBudgetPolicy>>,

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
        // Terminal state: no further effects are emitted.
        if matches!(session.state(), AgentState::Failure) {
            return vec![];
        }

        let prev_was_subroutine = matches!(session.state(), AgentState::SubRoutine(_));
        let prev_handle = match session.state() {
            AgentState::SubRoutine(h) => Some(h.clone()),
            _ => None,
        };

        // Step 0: Drain separate signal channels if a drain adapter is wired.
        // This preserves the canonical order mandated by SPECS.md §1.4.
        if let Some(ref drain) = self.signal_drain_order {
            let batch = drain.drain();
            let buffer = crate::SignalBuffer {
                system_signals: batch.system_signals,
                governance_notifications: batch.governance_notifications,
                async_task_results: batch.async_task_results,
            };
            session.extensions_mut().insert_hot(buffer);
        }

        // Step 1: Inject SessionSnapshot before each transition cycle.
        // `update_hot` reuses the pre-warmed slot to avoid per-hook allocations.
        let snapshot = session.snapshot();
        session.extensions_mut().update_hot(snapshot);

        let mut effects = Vec::new();

        // Step 2: EpochInterceptor (optional, but evaluated first if present).
        if let Some(ref interceptor) = self.epoch_interceptor {
            match interceptor.intercept_epoch(input, session.extensions_mut()) {
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

        // Step 3: SubRoutineHandler (optional).
        if let Some(ref handler) = self.subroutine_handler
            && let AgentState::SubRoutine(handle) = session.state()
        {
            match self.session_registry.get_mut(handle) {
                Some(child) => {
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
                        Ok(None) => {
                            // Sub-routine is still active; do not fall through to parent dispatch.
                            return self.finalize_transition(
                                session,
                                prev_was_subroutine,
                                prev_handle,
                                effects,
                            );
                        }
                        Err(err) => {
                            effects.push(self.plugin_fault("subroutine_handler", err));
                        }
                    }
                }
                None => {
                    // Defensive: child missing from registry is an isolation breach.
                    // Do not fall through to parent dispatch.
                    return self.finalize_transition(
                        session,
                        prev_was_subroutine,
                        prev_handle,
                        vec![
                            Effect::Error {
                                code: ErrorCode::StateInconsistency,
                                message: format!(
                                    "sub-routine {} missing from registry",
                                    handle.as_str()
                                ),
                            },
                            Effect::SystemIdle,
                        ],
                    );
                }
            }
        }

        // Step 4: on_input hook (routed).
        match self.eval_on_input(session, input) {
            InputResult::OverrideTransition(ov_effects, _source_plugin) => {
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

        // Step 5: Main dispatch on EngineInput.
        match self.dispatch_input(session, input) {
            Ok(dispatch_effects) => effects.extend(dispatch_effects),
            Err(err) => {
                effects.push(Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    message: err.to_string(),
                });
            }
        }

        // Steps 6-11: Finalize.
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
        for i in 0..self.routing_table.route_after_prediction.len() {
            let idx = self.routing_table.route_after_prediction[i];
            let name = self.plugins[idx].name();
            let snapshot = session.snapshot();
            session.extensions_mut().update_hot(snapshot);
            let result = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.after_prediction(session.extensions_mut())
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
            .filter(|i| active_mask.get(*i).copied().unwrap_or(false))
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
    /// The `cycle_rollback_policy` is moved into `ExtensionStorage` for the
    /// duration of the hook. `ExtensionStorage` calls `on_mutation` on first
    /// write, then `with_rollback` drives `commit_hook` or `rollback_hook`
    /// after the plugin returns. On panic, `catch_unwind` ensures the
    /// policy is restored to the engine before `resume_unwind` re-raises
    /// the panic.
    ///
    /// **Panic safety:** `catch_unwind` + `resume_unwind` is the Rust
    /// idiom for cleanup on panic. The panic is never swallowed; it is
    /// intercepted only to prevent `ExtensionStorage` from being left
    /// without its rollback policy. See PHILOSOPHY.md §2.4 (amended).
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
    ) -> R
    where
        R: IsResultErr,
    {
        let rollback = self.cycle_rollback_policy.take();
        session.extensions_mut().set_rollback_policy(rollback);
        session.extensions_mut().begin_hook();

        // Plugins are external code; they must not panic, but if they do
        // we must still restore the rollback policy before propagating.
        // AssertUnwindSafe is required because Session contains
        // PhantomData<*mut ()> which is !UnwindSafe by default.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self, session)));

        match result {
            Ok(r) => {
                if r.is_err() {
                    session.extensions_mut().rollback_hook();
                } else {
                    session.extensions_mut().commit_hook();
                }
                let rollback = session.extensions_mut().take_rollback_policy();
                self.cycle_rollback_policy = rollback;
                r
            }
            Err(panic_payload) => {
                session.extensions_mut().rollback_hook();
                let rollback = session.extensions_mut().take_rollback_policy();
                self.cycle_rollback_policy = rollback;
                std::panic::resume_unwind(panic_payload);
            }
        }
    }

    /// Evaluate the `on_input` route.
    ///
    /// `OverrideTransition` from the first plugin wins; subsequent ones are
    /// logged as superseded. `Block` short-circuits immediately.
    ///
    /// History edits are applied incrementally after each plugin so that
    /// subsequent plugins observe the mutated history. If an edit is
    /// invalid, evaluation stops and the error is surfaced as an
    /// `Effect::Error(StateInconsistency)`.
    fn eval_on_input(&mut self, session: &mut Session, input: &EngineInput) -> InputResult {
        let mut accumulated = Vec::new();
        let mut override_transition: Option<(Vec<Effect>, String)> = None;
        let mut superseded_traces: Vec<(String, PolicyDecision, String)> = Vec::new();

        for i in 0..self.routing_table.route_on_input.len() {
            let idx = self.routing_table.route_on_input[i];
            let name = self.plugins[idx].name();
            let snapshot = session.snapshot();
            session.extensions_mut().update_hot(snapshot);

            let decision = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_input(input, session.extensions_mut())
            });

            match decision {
                Ok(PolicyDecision::Allow) => {}
                Ok(PolicyDecision::Block { reason }) => {
                    // Flush accumulated traces before short-circuiting so
                    // that audit logs are not lost (I-Gov-OverrideTrace).
                    for (source, decision, preempted_by) in &superseded_traces {
                        self.log_superseded_transition(session, source, decision, preempted_by);
                    }
                    if let Some((ref effects, ref source)) = override_transition {
                        self.log_override_transition(session, source, effects);
                    }
                    return InputResult::Block { reason };
                }
                Ok(PolicyDecision::MutateHistory(edits)) => {
                    // Apply incrementally so the next plugin sees the
                    // mutated history. I-Gov-Decision-Isolation.
                    if let Err(err) = Self::apply_history_edits(session, &edits) {
                        accumulated.push(Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            message: err.to_string(),
                        });
                        // Stop evaluating further plugins: history is in
                        // an inconsistent state.
                        break;
                    }
                }
                Ok(PolicyDecision::RequestEffect(eff)) => {
                    accumulated.push(eff);
                }
                Ok(PolicyDecision::OverrideTransition(effects)) => {
                    if let Some((_, ref winner)) = override_transition {
                        let winner = winner.clone();
                        superseded_traces.push((
                            name.to_string(),
                            PolicyDecision::OverrideTransition(effects),
                            winner,
                        ));
                    } else {
                        override_transition = Some((effects, name.to_string()));
                    }
                }
                Err(err) => {
                    accumulated.push(self.plugin_fault(name, err));
                }
            }
        }

        // Flush trace logs after rollback windows are closed.
        for (source, decision, preempted_by) in superseded_traces {
            self.log_superseded_transition(session, &source, &decision, &preempted_by);
        }
        if let Some((ref effects, ref source)) = override_transition {
            self.log_override_transition(session, source, effects);
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
            EngineInput::RestoreSubRoutine { handle } => {
                self.dispatch_restore_subroutine(session, handle)
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

        session.push_history(ChatMessage::User {
            content: content.to_string(),
        });

        let generation_id = self.next_generation_id;
        self.next_generation_id += 1;

        // before_prediction hook: collect decisions.
        // History is cloned once outside the loop so that N plugins do
        // not perform N allocations (I-Core-ChunkBudget).
        let history = session.history().to_vec();
        let mut decisions = Vec::new();
        for i in 0..self.routing_table.route_before_prediction.len() {
            let idx = self.routing_table.route_before_prediction[i];
            let name = self.plugins[idx].name();
            let snapshot = session.snapshot();
            session.extensions_mut().update_hot(snapshot);
            let decision = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.before_prediction(&history, session.extensions_mut())
            });
            match decision {
                Ok(decision) => decisions.push(decision),
                Err(err) => {
                    effects.push(self.plugin_fault(name, err));
                }
            }
        }

        // DecisionAggregator (mandatory).
        match self
            .decision_aggregator
            .aggregate_decisions(decisions, session.extensions_mut())
        {
            Ok(decision) => match decision {
                PolicyDecision::Allow => {}
                PolicyDecision::Block { reason } => {
                    session.pop_history();
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
                    session.pop_history();
                    effects.extend(ov);
                    return Ok(effects);
                }
            },
            Err(err) => {
                effects.push(self.plugin_fault("decision_aggregator", err));
            }
        }

        session.push_state(AgentState::Predicting { generation_id })?;
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
    /// UTF-8 safety: tool argument chunks and assistant text are accumulated
    /// as raw bytes. Conversion to `String` happens only at materialization
    /// boundaries (`Done` / `ToolCallDone`), using `String::from_utf8` so
    /// that split multi-byte characters are not replaced with U+FFFD.
    ///
    /// # Complexity
    /// O(p + t) where p = plugins on `route_on_stream_event`, t = pending
    /// tool descriptors. One `BTreeMap` insertion per `ToolCallStart`.
    /// No heap allocation on `Hold`.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Core-ChunkBudget, I-Core-ActiveToolCall
    fn dispatch_llm_stream(
        &mut self,
        session: &mut Session,
        event: &StreamEvent,
    ) -> Result<Vec<Effect>, BriocheError> {
        let mut effects = Vec::new();

        if !matches!(session.state(), AgentState::Predicting { .. }) {
            return Ok(effects);
        }

        for i in 0..self.routing_table.route_on_stream_event.len() {
            let idx = self.routing_table.route_on_stream_event[i];
            let name = self.plugins[idx].name();
            let snapshot = session.snapshot();
            session.extensions_mut().update_hot(snapshot);
            let action = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_stream_event(event, session.extensions_mut())
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
                session.append_assistant_bytes(chunk);
            }
            StreamEvent::ToolCallStart { id, name, .. } => {
                session
                    .extensions_mut()
                    .with_or_insert_default::<StreamToolAccumulator, _>(|accumulator| {
                        accumulator.pending.insert(
                            id.clone(),
                            ToolCallDescriptor {
                                tool_id: id.clone(),
                                tool_name: name.clone(),
                                arguments: String::new(),
                                timeout_ms: None,
                            },
                        );
                        accumulator
                            .pending_args_bytes
                            .insert(id.clone(), Vec::new());
                    });
            }
            StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
                session
                    .extensions_mut()
                    .with_or_insert_default::<StreamToolAccumulator, _>(|accumulator| {
                        if let Some(bytes) = accumulator.pending_args_bytes.get_mut(id) {
                            // Accumulate raw bytes; conversion to String happens
                            // only at the ToolCallDone materialization boundary.
                            bytes.extend_from_slice(chunk);
                        }
                    });
            }
            StreamEvent::ToolCallDone { .. } => {
                // Persist any assistant text that preceded the tool calls.
                if !session.is_pending_assistant_empty() {
                    let text = Self::materialize_assistant_text(session)?;
                    session.push_history(ChatMessage::Assistant { content: text });
                    session.clear_assistant_bytes();
                }

                // Prediction completes before tool execution.
                effects.extend(self.eval_after_prediction(session));

                // Drain all pending descriptors, convert accumulated
                // bytes to String, and materialize them.
                let mut pending: Vec<ToolCallDescriptor> = session
                    .extensions_mut()
                    .with_or_insert_default::<StreamToolAccumulator, _>(
                    |accumulator| -> Result<Vec<ToolCallDescriptor>, BriocheError> {
                        // Convert raw bytes to String at the materialization
                        // boundary so split multi-byte UTF-8 chars are preserved.
                        for (tool_id, bytes) in &accumulator.pending_args_bytes {
                            if let Some(descriptor) = accumulator.pending.get_mut(tool_id) {
                                descriptor.arguments =
                                    String::from_utf8(bytes.clone()).map_err(|e| {
                                        BriocheError::InvalidStateTransition(format!(
                                            "tool {} argument bytes are invalid UTF-8: {e}",
                                            tool_id
                                        ))
                                    })?;
                            }
                        }
                        accumulator.pending_args_bytes.clear();
                        Ok(std::mem::take(&mut accumulator.pending)
                            .into_values()
                            .collect())
                    },
                )?;
                if !pending.is_empty() {
                    self.handle_tool_calls(session, &mut pending, &mut effects)?;
                    let (active, err_effect) = self.materialize_tool_calls(pending);
                    if let Some(err) = err_effect {
                        effects.push(err);
                    }
                    session.set_active_tools(active.clone());
                    let generation_id =
                        if let AgentState::Predicting { generation_id } = session.state() {
                            *generation_id
                        } else {
                            return Err(BriocheError::InvalidStateTransition(
                                "expected Predicting state when materializing tool calls".into(),
                            ));
                        };
                    session.push_state(AgentState::ExecutingTools { generation_id })?;
                    effects.push(Effect::ExecuteTools(active));
                    effects.push(Effect::SaveSession);
                }
            }
            StreamEvent::Done => {
                // Persist accumulated assistant text before returning to Idle.
                if !session.is_pending_assistant_empty() {
                    let text = Self::materialize_assistant_text(session)?;
                    session.push_history(ChatMessage::Assistant { content: text });
                    session.clear_assistant_bytes();
                }

                // Prediction completes without tool calls.
                effects.extend(self.eval_after_prediction(session));

                if matches!(session.state(), AgentState::Predicting { .. }) {
                    session.pop_state()?;
                    effects.push(Effect::SystemIdle);
                    effects.push(Effect::SaveSession);
                }
            }
        }

        Ok(effects)
    }

    /// Convert accumulated assistant bytes to a `String` at a safe
    /// materialization boundary, returning an error if the bytes are not
    /// valid UTF-8.
    ///
    /// Using `String::from_utf8` instead of `from_utf8_lossy` prevents
    /// silent replacement of split multi-byte characters with U+FFFD.
    fn materialize_assistant_text(session: &mut Session) -> Result<String, BriocheError> {
        let bytes = std::mem::take(session.pending_assistant_bytes_mut());
        String::from_utf8(bytes).map_err(|e| {
            BriocheError::InvalidStateTransition(format!(
                "assistant stream contained invalid UTF-8: {e}"
            ))
        })
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
        for i in 0..self.routing_table.route_on_tool_calls.len() {
            let idx = self.routing_table.route_on_tool_calls[i];
            let name = self.plugins[idx].name();
            let snapshot = session.snapshot();
            session.extensions_mut().update_hot(snapshot);
            let result = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_tool_calls(descriptors, session.extensions_mut())
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
        let mut missing = false;
        let active = descriptors
            .into_iter()
            .map(|d| {
                let timeout_ms = d.timeout_ms.unwrap_or_else(|| {
                    missing = true;
                    self.default_tool_timeout_ms
                });
                ActiveToolCall {
                    tool_id: d.tool_id,
                    tool_name: d.tool_name,
                    arguments: d.arguments,
                    timeout_ms,
                }
            })
            .collect();
        let effect = if missing {
            Some(Effect::Error {
                code: ErrorCode::StateInconsistency,
                message: "Missing timeout, applied default".into(),
            })
        } else {
            None
        };
        (active, effect)
    }

    /// Dispatch `ToolCallsResult` input.
    ///
    /// The state stack is modified only after the aggregator confirms `Allow`.
    /// This prevents the double-pop bug that previously occurred when the
    /// aggregator returned `Block` or `OverrideTransition`.
    fn dispatch_tool_calls_result(
        &mut self,
        session: &mut Session,
        generation_id: u64,
        results: &[ToolResultDTO],
    ) -> Result<Vec<Effect>, BriocheError> {
        let mut effects = Vec::new();

        // State validation: ToolCallsResult should only arrive while the
        // session is in ExecutingTools (or SubRoutine for backward-compat
        // paths without a registered handler).
        // Validate generation_id to prevent stale async responses from corrupting
        // the current prediction cycle. I-Core-Pure, I-Gov-Epoch-Reject.
        let expected_generation = match session.state() {
            AgentState::ExecutingTools { generation_id } => *generation_id,
            AgentState::SubRoutine(_) => {
                // Sub-routine path: generation validation is delegated to the
                // SubRoutineHandler if present; otherwise we accept any generation
                // to preserve backward-compat.
                generation_id
            }
            _ => {
                return Ok(vec![Effect::Error {
                    code: ErrorCode::StateInconsistency,
                    message: format!(
                        "ToolCallsResult received in {:?} state; expected ExecutingTools",
                        session.state()
                    ),
                }]);
            }
        };

        if generation_id != expected_generation {
            return Ok(vec![
                Effect::Error {
                    code: ErrorCode::EpochMismatch,
                    message: format!(
                        "ToolCallsResult generation_id {} does not match expected {}",
                        generation_id, expected_generation
                    ),
                },
                Effect::SystemIdle,
            ]);
        }

        // on_tool_result hook: in-place mutation.
        let mut mutable_results = results.to_vec();
        for i in 0..self.routing_table.route_on_tool_result.len() {
            let idx = self.routing_table.route_on_tool_result[i];
            let name = self.plugins[idx].name();
            let snapshot = session.snapshot();
            session.extensions_mut().update_hot(snapshot);
            let result = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.on_tool_result(&mut mutable_results, session.extensions_mut())
            });
            if let Err(err) = result {
                effects.push(self.plugin_fault(name, err));
            }
        }

        // Push results into history.
        for result in &mutable_results {
            session.push_history(ChatMessage::ToolResult {
                id: result.tool_id.clone(),
                tool_name: result.tool_name.clone(),
                outcome: result.outcome.clone(),
            });
        }

        // before_prediction hook: collect decisions (re-prediction path).
        // History cloned once outside the loop to avoid O(n×p) allocations.
        let history = session.history().to_vec();
        let mut decisions = Vec::new();
        for i in 0..self.routing_table.route_before_prediction.len() {
            let idx = self.routing_table.route_before_prediction[i];
            let name = self.plugins[idx].name();
            let snapshot = session.snapshot();
            session.extensions_mut().update_hot(snapshot);
            let decision = self.with_rollback(session, name, |engine, session| {
                let plugin = &engine.plugins[idx];
                plugin.before_prediction(&history, session.extensions_mut())
            });
            match decision {
                Ok(decision) => decisions.push(decision),
                Err(err) => {
                    effects.push(self.plugin_fault(name, err));
                }
            }
        }

        // DecisionAggregator (mandatory). Do NOT mutate state until Allow.
        match self
            .decision_aggregator
            .aggregate_decisions(decisions, session.extensions_mut())
        {
            Ok(decision) => match decision {
                PolicyDecision::Allow => {}
                PolicyDecision::Block { reason } => {
                    // Roll back history insertions; leave stack intact.
                    for _ in results {
                        session.pop_history();
                    }
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
                    // Roll back history insertions; leave stack intact.
                    for _ in results {
                        session.pop_history();
                    }
                    effects.extend(ov);
                    return Ok(effects);
                }
            },
            Err(err) => {
                effects.push(self.plugin_fault("decision_aggregator", err));
            }
        }

        // Now safe to transition: pop ExecutingTools. The stack already
        // contains Predicting underneath, so we do NOT push it again.
        session.pop_state()?;
        session.clear_active_tools();

        effects.push(Effect::CallLlmNetwork);
        effects.push(Effect::SaveSession);

        Ok(effects)
    }

    /// Dispatch `RestoreSubRoutine` input.
    fn dispatch_restore_subroutine(
        &mut self,
        session: &mut Session,
        handle: &SubRoutineHandle,
    ) -> Result<Vec<Effect>, BriocheError> {
        if matches!(session.state(), AgentState::Failure) {
            return Err(BriocheError::InvalidStateTransition(
                "cannot restore sub-routine while in Failure state".into(),
            ));
        }
        let child = Session::new(handle.as_str());
        let _previous = self.session_registry.insert(handle.clone(), child);

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
        // Step 6: SubRoutineLifecycleGuard (mandatory if exiting SubRoutine).
        if prev_was_subroutine
            && let Some(handle) = prev_handle
            && !matches!(session.state(), AgentState::SubRoutine(_))
        {
            match self.subroutine_lifecycle_guard.on_exit(
                handle,
                session,
                &mut self.session_registry,
            ) {
                Ok(guard_effects) => effects.extend(guard_effects),
                Err(err) => {
                    effects.push(self.plugin_fault("subroutine_lifecycle_guard", err));
                }
            }
        }

        // Step 7: Hook effect validation (optional).
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
                    let variant = Self::effect_variant_name(effect);
                    if !constraint.is_allowed_fallback("transition", variant) {
                        *effect = Effect::Error {
                            code: ErrorCode::StateInconsistency,
                            message: "Effect not allowed on this hook".into(),
                        };
                    }
                }
            }
        }

        // Step 8: on_error hook for PluginFault effects (optional).
        let mut error_hook_effects = Vec::new();
        for effect in &effects {
            if let Effect::PluginFault { error, .. } = effect {
                for i in 0..self.routing_table.route_on_error.len() {
                    let idx = self.routing_table.route_on_error[i];
                    let name = self.plugins[idx].name();
                    let snapshot = session.snapshot();
                    session.extensions_mut().update_hot(snapshot);
                    let decision = self.with_rollback(session, name, |engine, session| {
                        let plugin = &engine.plugins[idx];
                        plugin.on_error(error, session.extensions_mut())
                    });
                    match decision {
                        Ok(PolicyDecision::Allow) => {}
                        Ok(PolicyDecision::Block { reason }) => {
                            error_hook_effects.push(Effect::Error {
                                code: ErrorCode::StateInconsistency,
                                message: format!("on_error block from {}: {}", name, reason),
                            });
                        }
                        Ok(PolicyDecision::OverrideTransition(ov)) => {
                            error_hook_effects.extend(ov);
                        }
                        Ok(PolicyDecision::RequestEffect(eff)) => {
                            error_hook_effects.push(eff);
                        }
                        Ok(PolicyDecision::MutateHistory(edits)) => {
                            if let Err(err) = Self::apply_history_edits(session, &edits) {
                                error_hook_effects.push(Effect::Error {
                                    code: ErrorCode::StateInconsistency,
                                    message: err.to_string(),
                                });
                            }
                        }
                        Err(err) => {
                            error_hook_effects.push(self.plugin_fault(name, err));
                        }
                    }
                }
            }
        }
        effects.extend(error_hook_effects);

        // Step 9: ConsistencyVerifier (optional).
        if let Some(ref verifier) = self.consistency_verifier {
            match verifier.verify_consistency(session) {
                Ok(report) => {
                    if let Some(new_state) = report.suggested_state {
                        session.set_state(new_state);
                        if report.clear_stack {
                            session.clear_state_stack();
                        }
                    }
                    effects.extend(report.effects);
                }
                Err(err) => {
                    effects.push(self.plugin_fault("consistency_verifier", err));
                }
            }
        }

        // Step 10: GovernanceFailoverHandler (optional).
        // Preserve the original PluginFault for audit; append failover
        // effects rather than replacing them.
        if let Some(ref handler) = self.governance_failover_handler {
            let mut augmented_effects = Vec::new();
            let mut has_fault = false;
            for effect in &effects {
                if let Effect::PluginFault { .. } = effect {
                    has_fault = true;
                    match handler.handle_failure(session, effect) {
                        Ok(Some(failover)) => {
                            // Keep the original fault so telemetry retains
                            // the plugin name and error message.
                            augmented_effects.push(effect.clone());
                            augmented_effects.extend(failover);
                        }
                        _ => {
                            augmented_effects.push(effect.clone());
                        }
                    }
                } else {
                    augmented_effects.push(effect.clone());
                }
            }
            if has_fault {
                effects = augmented_effects;
            }
        }

        // Step 11: RebuildRoutes last position guarantee — MUST be final.
        Self::ensure_rebuildroutes_last(&mut effects);

        effects
    }

    // -----------------------------------------------------------------------
    // Static helpers
    // -----------------------------------------------------------------------

    /// Apply a sequence of `HistoryEdit`s to the session atomically.
    ///
    /// All edits are validated before any mutation occurs. If any edit
    /// is invalid, the session history is left unchanged.
    fn apply_history_edits(
        session: &mut Session,
        edits: &[HistoryEdit],
    ) -> Result<(), BriocheError> {
        // First pass: validate all edits.
        for edit in edits {
            match edit {
                HistoryEdit::Insert { index, .. } => {
                    if *index > session.history_len() {
                        return Err(BriocheError::InvalidStateTransition(format!(
                            "history insert index {} out of bounds (len={})",
                            index,
                            session.history_len()
                        )));
                    }
                }
                HistoryEdit::Replace { index, .. } => {
                    if *index >= session.history_len() {
                        return Err(BriocheError::InvalidStateTransition(format!(
                            "history replace index {} out of bounds (len={})",
                            index,
                            session.history_len()
                        )));
                    }
                }
                HistoryEdit::Truncate { .. } => {}
            }
        }
        // Second pass: apply edits using unchecked methods because
        // bounds were already validated in the first pass.
        for edit in edits {
            match edit {
                HistoryEdit::Insert { index, message } => {
                    // SAFETY: validated in first pass above.
                    unsafe { session.insert_history_unchecked(*index, message.clone()) };
                }
                HistoryEdit::Replace { index, message } => {
                    // SAFETY: validated in first pass above.
                    unsafe { session.replace_history_unchecked(*index, message.clone()) };
                }
                HistoryEdit::Truncate { keep_last } => {
                    session.truncate_history(*keep_last);
                }
            }
        }
        Ok(())
    }

    /// Guarantee that `RebuildRoutes` occupies the last position in effects.
    ///
    /// If multiple `RebuildRoutes` are present, all but one are removed
    /// and the survivor is placed at the end. Any effects that appear after
    /// the last `RebuildRoutes` are truncated — by contract `RebuildRoutes`
    /// must be the final effect in the returned vector.
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

    /// Returns the canonical variant name of an `Effect`.
    ///
    /// **Keep in sync with `effect_to_bitmask`** in `types.rs`. Both
    /// functions perform exhaustive matching; adding a new `Effect`
    /// variant requires updating both.
    fn effect_variant_name(effect: &Effect) -> &'static str {
        match effect {
            Effect::CallLlmNetwork => "CallLlmNetwork",
            Effect::ExecuteTools(_) => "ExecuteTools",
            Effect::ForwardToUi(_) => "ForwardToUi",
            Effect::Error { .. } => "Error",
            Effect::SaveSession => "SaveSession",
            Effect::SavePluginBlob { .. } => "SavePluginBlob",
            Effect::TriggerSummarization => "TriggerSummarization",
            Effect::ExecuteCpuTask { .. } => "ExecuteCpuTask",
            Effect::TriggerGc => "TriggerGc",
            Effect::SystemIdle => "SystemIdle",
            Effect::PluginFault { .. } => "PluginFault",
            Effect::RebuildRoutes => "RebuildRoutes",
            Effect::SubRoutineRestored { .. } => "SubRoutineRestored",
        }
    }

    // -----------------------------------------------------------------------
    // Trace logging
    // -----------------------------------------------------------------------

    fn log_override_transition(
        &self,
        session: &mut Session,
        source_plugin: &str,
        effects: &[Effect],
    ) {
        let epoch = session
            .extensions_mut()
            .with_or_insert_default::<EpochState, _>(|state| state.current_generation);
        session
            .extensions_mut()
            .with_or_insert_default::<TransitionTraceLog, _>(|log| {
                if log.entries.len() >= TRACE_LOG_CAPACITY {
                    log.entries.pop_front();
                }
                log.entries.push_back(TransitionTrace {
                    source_plugin: source_plugin.to_string(),
                    decision: PolicyDecision::OverrideTransition(effects.to_vec()),
                    epoch,
                });
            });
    }

    fn log_superseded_transition(
        &self,
        session: &mut Session,
        source_plugin: &str,
        attempted_decision: &PolicyDecision,
        preempted_by: &str,
    ) {
        let epoch = session
            .extensions_mut()
            .with_or_insert_default::<EpochState, _>(|state| state.current_generation);
        session
            .extensions_mut()
            .with_or_insert_default::<SupersededTransitionTraceLog, _>(|log| {
                if log.entries.len() >= TRACE_LOG_CAPACITY {
                    log.entries.pop_front();
                }
                log.entries.push_back(SupersededTransitionTrace {
                    source_plugin: source_plugin.to_string(),
                    attempted_decision: attempted_decision.clone(),
                    preempted_by: preempted_by.to_string(),
                    epoch,
                });
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
    signal_drain_order: Option<Box<dyn SignalDrainOrder>>,
    cow_budget_policy: Option<Box<dyn CowBudgetPolicy>>,
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
            signal_drain_order: None,
            cow_budget_policy: None,
            default_tool_timeout_ms: crate::DEFAULT_TOOL_TIMEOUT_MS,
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

    /// Inject a `SignalDrainOrder` adapter so `transition()` automatically
    /// drains separate signal channels before each cycle.
    ///
    /// Refs: SPECS.md §1.4, I-Shell-Drain-Atomic
    pub fn with_signal_drain_order(mut self, drain: Box<dyn SignalDrainOrder>) -> Self {
        self.signal_drain_order = Some(drain);
        self
    }

    /// Inject a `CowBudgetPolicy` to provide per-hook COW budgets to
    /// `CycleRollbackPolicy` implementations.
    ///
    /// Refs: SPECS.md §2.11
    pub fn with_cow_budget_policy(mut self, policy: Box<dyn CowBudgetPolicy>) -> Self {
        self.cow_budget_policy = Some(policy);
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
    /// Returns `Err` if a mandatory trait is missing, if plugin names are
    /// not unique, or if any plugin name is empty.
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

        // Enforce unique plugin names for deterministic routing.
        let mut seen_names = std::collections::BTreeSet::new();
        for plugin in &self.plugins {
            let name = plugin.name();
            if name.is_empty() {
                return Err(BriocheError::Other("plugin name must not be empty".into()));
            }
            if !seen_names.insert(name) {
                return Err(BriocheError::Other(format!(
                    "duplicate plugin name: {}",
                    name
                )));
            }
        }

        if self.default_tool_timeout_ms == 0 {
            return Err(BriocheError::Other(
                "default_tool_timeout_ms must be > 0".into(),
            ));
        }

        let routing_table = UnifiedRoutingTable::from_plugins(&self.plugins);

        Ok(BriocheEngine {
            plugins: self.plugins,
            routing_table,
            epoch_interceptor: self.epoch_interceptor,
            subroutine_handler: self.subroutine_handler,
            consistency_verifier: self.consistency_verifier,
            decision_aggregator,
            hook_effect_constraint: self.hook_effect_constraint,
            cycle_rollback_policy: self.cycle_rollback_policy,
            subroutine_lifecycle_guard,
            governance_failover_handler: self.governance_failover_handler,
            signal_drain_order: self.signal_drain_order,
            cow_budget_policy: self.cow_budget_policy,
            session_registry: SessionRegistry::new(),
            next_generation_id: crate::INITIAL_GENERATION_ID,
            default_tool_timeout_ms: self.default_tool_timeout_ms,
            _not_send_sync: std::marker::PhantomData,
        })
    }
}
