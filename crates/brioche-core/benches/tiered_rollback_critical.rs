//! Benchmark: `tiered_rollback_critical` — Sprint 18.
//!
//! Target: < 5 µs for `TieredUndoFrameGuard::rollback_hook` on a
//! `#[brioche(critical_state)]` type.
//!
//! Refs: I-Gov-Rollback-Critical

use brioche_core::{BriocheExtensionType, CycleRollbackPolicy, EpochState, ExtensionStorage};
use brioche_governance_default::TieredUndoFrameGuard;
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_tiered_rollback_critical(c: &mut Criterion) {
    let mut guard = TieredUndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    assert!(
        ext.insert(EpochState {
            current_generation: 7,
        })
        .is_ok()
    );

    c.bench_function("tiered_rollback_critical", |b| {
        b.iter(|| {
            guard.begin_hook();

            let type_id = std::any::TypeId::of::<EpochState>();
            let vtable = EpochState::build_vtable();
            ext.with_or_insert_default::<EpochState, _>(|current| {
                guard.on_mutation(type_id, &vtable, current);
                current.current_generation = 777;
            });

            guard.rollback_hook(&mut ext);

            ext.with_or_insert_default::<EpochState, _>(|restored| {
                assert_eq!(restored.current_generation, 7);
            });
        });
    });
}

criterion_group!(benches, bench_tiered_rollback_critical);
criterion_main!(benches);
