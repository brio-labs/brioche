//! Benchmark suite for `brioche-governance-default` — governance hot paths.
//!
//! Replaces the previous Criterion suite with Divan, adding parameterized
//! benchmarks for `NegotiationBroker`.
//!
//! ## Framework note
//! Uses **Divan** (`=0.1.21`) for lightweight, attribute-based benchmarks.
//! Divan is currently unmaintained (last commit April 2025), but the API is
//! stable and the compile-time overhead is minimal. Pinned to avoid accidental
//! upgrades. If Divan breaks in a future Rust edition, evaluate Criterion or
//! a custom harness as fallback.
//!
//! ## Benchmark groups
//! - `governance`: `NegotiationBroker` decision aggregation.
//!
//! Refs: docs/SPECS.md §Pillar 3, PHILOSOPHY.md §2.3

use brioche_core::{DecisionAggregator, Effect, ExtensionStorage, PolicyDecision};
use brioche_governance_default::NegotiationBroker;

// ---------------------------------------------------------------------------
// Group: governance — NegotiationBroker
// ---------------------------------------------------------------------------

/// Benchmark: `negotiation_broker` — decision aggregation with varying counts.
///
/// Target: < 50 µs for `NegotiationBroker::aggregate_decisions`.
///
/// Refs: I-Gov-Decision-Required
#[divan::bench_group(name = "governance", sample_count = 10000)]
mod governance {
    use super::*;

    #[divan::bench(args = [1usize, 3, 5, 10])]
    fn negotiation_broker(bencher: divan::Bencher, decision_count: usize) {
        let broker = NegotiationBroker::new();

        bencher
            .with_inputs(|| {
                let ext = ExtensionStorage::new();
                let mut decisions = Vec::new();
                for i in 0..decision_count {
                    if i % 3 == 0 {
                        decisions.push(PolicyDecision::Allow);
                    } else if i % 3 == 1 {
                        decisions.push(PolicyDecision::RequestEffect(Effect::SaveSession));
                    } else {
                        decisions.push(PolicyDecision::Block {
                            reason: "test".into(),
                        });
                    }
                }
                (ext, decisions)
            })
            .bench_local_refs(|(ext, decisions)| {
                let result = broker.aggregate_decisions(decisions.clone(), ext);
                assert!(result.is_ok());
            });
    }
}

fn main() {
    divan::main();
}
