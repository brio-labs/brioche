//! Benchmark: `negotiation_broker` — Sprint 18.
//!
//! Target: < 50 µs for `NegotiationBroker::aggregate_decisions`.
//!
//! Refs: I-Gov-Decision-Required

use brioche_core::{DecisionAggregator, Effect, ExtensionStorage, PolicyDecision};
use brioche_governance_default::NegotiationBroker;
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_negotiation_broker(c: &mut Criterion) {
    let broker = NegotiationBroker::new();
    let mut ext = ExtensionStorage::new();

    let decisions = vec![
        PolicyDecision::Allow,
        PolicyDecision::RequestEffect(Effect::SaveSession),
        PolicyDecision::Allow,
    ];

    c.bench_function("negotiation_broker", |b| {
        b.iter(|| {
            let result = broker.aggregate_decisions(decisions.clone(), &mut ext);
            assert!(result.is_ok());
        });
    });
}

criterion_group!(benches, bench_negotiation_broker);
criterion_main!(benches);
