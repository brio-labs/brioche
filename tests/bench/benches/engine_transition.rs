//! Criterion benchmark for cross-crate engine transitions.
//!
//! Measures the synchronous `BriocheEngine::transition()` path when wired
//! with the standard governance profile.
//!
//! Refs: I-Core-NoPanic, I-Core-Pure

use brioche_core::{BriocheEngineBuilder, EngineInput, Session};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
use criterion::{Criterion, criterion_group, criterion_main};

/// Build an engine using the standard governance profile.
fn build_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Standard)
        .build()
}

/// Benchmark user-message transitions.
fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("transition_user_message", |b| {
        let mut engine = build_engine();
        let mut session = Session::new("bench");
        let input = EngineInput::UserMessage("hello world".into());
        b.iter(|| engine.transition(&mut session, &input));
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
