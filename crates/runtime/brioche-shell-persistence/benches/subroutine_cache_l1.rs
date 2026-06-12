//! Benchmark: `subroutine_cache_l1` — Sprint 18.
//!
//! Target: < 1 µs for L1 lookup in `SubRoutineCache`.
//!
//! Refs: I-Persist-Cache

use std::num::NonZeroUsize;

use brioche_shell_persistence::{
    FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion, SubRoutineCache,
};
use criterion::{Criterion, criterion_group, criterion_main};

fn make_dto(id: &str) -> SessionHeadDTO {
    SessionHeadDTO {
        version: SessionSchemaVersion::V1,
        id: id.into(),
        parent_id: None,
        state: FlattenedAgentState::Idle,
        state_stack: vec![],
        extensions: std::collections::BTreeMap::new(),
        persisted_msg_count: 0,
        compaction_index: 0,
    }
}

fn bench_subroutine_cache_l1(c: &mut Criterion) {
    let mut cache = SubRoutineCache::new(NonZeroUsize::MIN.saturating_add(99));

    // Populate L2, then promote to L1.
    for i in 0..20usize {
        let dto = make_dto(&format!("sub-{i}"));
        cache.insert(format!("sub-{i}"), dto);
        cache.promote_to_l1(format!("sub-{i}"));
    }

    c.bench_function("subroutine_cache_l1", |b| {
        b.iter(|| {
            let result = cache.get("sub-10");
            assert!(result.is_some());
        });
    });
}

criterion_group!(benches, bench_subroutine_cache_l1);
criterion_main!(benches);
