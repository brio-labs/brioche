//! Benchmark: `hook_effect_o1` — Sprint 18.
//!
//! Target: < 100 ns for `HookEffectConstraint::is_allowed_fast`.
//!
//! Refs: I-Core-HookEffect-O1

use brioche_core::{EffectBit, HookEffectConstraint};
use brioche_governance_default::FastHookEffectConstraint;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn bench_hook_effect_o1(c: &mut Criterion) {
    let mut masks = [0u64; 8];
    masks[0] = EffectBit::CALL_LLM_NETWORK | EffectBit::SAVE_SESSION | EffectBit::SYSTEM_IDLE;
    masks[1] = EffectBit::EXECUTE_TOOLS | EffectBit::SAVE_SESSION;
    let constraint = FastHookEffectConstraint::new(masks);

    c.bench_function("hook_effect_o1", |b| {
        b.iter(|| {
            let allowed = constraint.is_allowed_fast(0, black_box(EffectBit::CALL_LLM_NETWORK));
            black_box(allowed);
        });
    });
}

criterion_group!(benches, bench_hook_effect_o1);
criterion_main!(benches);
