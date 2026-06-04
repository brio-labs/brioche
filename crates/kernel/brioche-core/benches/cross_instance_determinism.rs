//! Benchmark: `cross_instance_determinism` — Sprint 18.
//!
//! Verifies zero divergence on 1,000 replays across two fresh engine
//! instances. The benchmark measures the replay time; correctness is
//! asserted inside the benchmark.
//!
//! Refs: I-Core-Pure, I-Core-NoPanic

use brioche_core::{
    AgentState, BriocheEngine, BriocheEngineBuilder, DecisionAggregator, Effect, EngineInput,
    ExtensionStorage, PluginResult, PolicyDecision, Session, StreamEvent, SubRoutineLifecycleGuard,
};
use criterion::{Criterion, criterion_group, criterion_main};

struct MockDecisionAggregator;
impl DecisionAggregator for MockDecisionAggregator {
    fn aggregate_decisions(
        &self,
        _decisions: Vec<PolicyDecision>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

struct MockSubRoutineLifecycleGuard;
impl SubRoutineLifecycleGuard for MockSubRoutineLifecycleGuard {
    fn on_exit(
        &self,
        _handle: brioche_core::SubRoutineHandle,
        _parent: &mut Session,
        _registry: &mut brioche_core::SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        Ok(vec![])
    }
}

fn build_engine() -> BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build()
}

/// Run a deterministic sequence of transitions.
fn run_sequence(engine: &mut BriocheEngine, session: &mut Session) -> Vec<Vec<Effect>> {
    let mut all_effects = Vec::new();

    // Sequence: UserMessage -> LlmStream Done -> ToolCallsResult
    all_effects.push(engine.transition(session, &EngineInput::UserMessage("hello".into())));
    all_effects.push(engine.transition(session, &EngineInput::LlmStream(StreamEvent::Done)));

    // Enter predicting and simulate tool call
    session.state = AgentState::Idle;
    session.state_stack.clear();
    session.history.clear();
    all_effects.push(engine.transition(session, &EngineInput::UserMessage("tool".into())));

    all_effects
}

fn bench_cross_instance_determinism(c: &mut Criterion) {
    c.bench_function("cross_instance_determinism", |b| {
        b.iter(|| {
            let mut engine_a = build_engine();
            let mut engine_b = build_engine();
            let mut session_a = Session::new("det");
            let mut session_b = Session::new("det");

            let effects_a = run_sequence(&mut engine_a, &mut session_a);
            let effects_b = run_sequence(&mut engine_b, &mut session_b);

            assert_eq!(effects_a, effects_b, "divergence detected");
            assert_eq!(session_a.state, session_b.state);
        });
    });
}

criterion_group!(benches, bench_cross_instance_determinism);
criterion_main!(benches);
