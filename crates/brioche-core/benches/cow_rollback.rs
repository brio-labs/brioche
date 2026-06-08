//! Benchmark: `cow_rollback` — Sprint 18.
//!
//! Target: < 10 µs for `UndoFrameGuard::rollback_hook`.
//!
//! Refs: I-Gov-Rollback-BestEffort

use brioche_core::{BriocheExtensionType, CycleRollbackPolicy, EpochState, ExtensionStorage};
use brioche_governance_default::UndoFrameGuard;
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_cow_rollback(c: &mut Criterion) {
    let mut guard = UndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    assert!(
        ext.insert(EpochState {
            current_generation: 42,
        })
        .is_ok()
    );

    c.bench_function("cow_rollback", |b| {
        b.iter(|| {
            guard.begin_hook();

            let type_id = std::any::TypeId::of::<EpochState>();
            let vtable = EpochState::build_vtable();
            ext.with_or_insert_default::<EpochState, _>(|current| {
                guard.on_mutation(type_id, &vtable, current);
                current.current_generation = 999;
            });

            guard.rollback_hook(&mut ext);

            ext.with_or_insert_default::<EpochState, _>(|restored| {
                assert_eq!(restored.current_generation, 42);
            });
        });
    });
}

criterion_group!(benches, bench_cow_rollback);
criterion_main!(benches);
