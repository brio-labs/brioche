//! Benchmark: `cow_rollback` — Sprint 18.
//!
//! Target: < 10 µs for `AdaptiveUndoFrameGuard::rollback_hook`.
//!
//! Refs: I-Gov-Rollback-BestEffort

use brioche_core::{BriocheExtensionType, CycleRollbackPolicy, EpochState, ExtensionStorage};
use brioche_governance_default::AdaptiveUndoFrameGuard;
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_cow_rollback(c: &mut Criterion) {
    let mut guard = AdaptiveUndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    ext.insert(EpochState {
        current_generation: 42,
    });

    c.bench_function("cow_rollback", |b| {
        b.iter(|| {
            guard.begin_hook();

            let type_id = std::any::TypeId::of::<EpochState>();
            let vtable = EpochState::build_vtable();
            let current = ext.get_or_insert_default::<EpochState>();
            guard.on_mutation(type_id, &vtable, current);

            current.current_generation = 999;

            guard.rollback_hook(&mut ext);

            let restored = ext.get_or_insert_default::<EpochState>();
            assert_eq!(restored.current_generation, 42);
        });
    });
}

criterion_group!(benches, bench_cow_rollback);
criterion_main!(benches);
