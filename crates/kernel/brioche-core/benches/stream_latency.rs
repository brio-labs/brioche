//! Benchmark: `stream_latency` — Sprint 18.
//!
//! Target: P99 < 50 µs for stream event processing via `transition()`.
//!
//! Refs: I-Core-StreamNoBranch, I-Core-ChunkBudget

use brioche_core::{
    AgentState, BriocheEngineBuilder, DecisionAggregator, Effect, EngineInput, ExecutionPath,
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

fn ok_or_abort<T, E>(result: Result<T, E>) -> T {
    match result {
        Ok(v) => v,
        Err(_) => std::process::abort(),
    }
}

fn bench_stream_latency(c: &mut Criterion) {
    let mut engine = ok_or_abort(
        BriocheEngineBuilder::new()
            .with_decision_aggregator(Box::new(MockDecisionAggregator))
            .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
            .build(),
    );

    let mut session = Session::new("bench");
    ok_or_abort(session.push_state(AgentState::Predicting { generation_id: 1 }));

    let event = StreamEvent::TextChunk {
        path: ExecutionPath::default(),
        chunk: bytes::Bytes::from_static(b"hello world"),
    };

    c.bench_function("stream_latency", |b| {
        b.iter(|| {
            let _effects = engine.transition(&mut session, &EngineInput::LlmStream(event.clone()));
        });
    });
}

criterion_group!(benches, bench_stream_latency);
criterion_main!(benches);
