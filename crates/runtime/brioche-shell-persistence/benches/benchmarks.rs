//! Benchmark suite for `brioche-shell-persistence` — persistence hot paths.
//!
//! Replaces the previous Criterion suite with Divan, adding parameterized
//! benchmarks for `SubRoutineCache` and `SessionHeadDTO` serialization.
//!
//! ## Framework note
//! Uses **Divan** (`=0.1.21`) for lightweight, attribute-based benchmarks.
//! Divan is currently unmaintained (last commit April 2025), but the API is
//! stable and the compile-time overhead is minimal. Pinned to avoid accidental
//! upgrades. If Divan breaks in a future Rust edition, evaluate Criterion or
//! a custom harness as fallback.
//!
//! ## Benchmark groups
//! - `persistence`: `SessionHeadDTO` roundtrip, `SubRoutineCache` L1/L2 lookup.
//!
//! Refs: docs/SPECS.md §Pillar 3, PHILOSOPHY.md §2.3

use std::num::NonZeroUsize;

use brioche_core::{AgentState, Session};
use brioche_shell_persistence::{
    FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion, SubRoutineCache, deserialize_head,
    serialize_head,
};

// ---------------------------------------------------------------------------
// Group: persistence — storage and cache
// ---------------------------------------------------------------------------

/// Benchmark: `redb_idempotence` — zero divergence between serializations.
///
/// Verifies that two serializations of the same session produce identical
/// bytes, and deserialization roundtrips correctly.
///
/// Refs: I-Persist-Idempotence
#[divan::bench_group(name = "persistence", sample_count = 10000)]
mod persistence {
    use super::*;

    #[divan::bench]
    fn redb_idempotence(bencher: divan::Bencher) {
        let mut session = Session::new("idempotent");
        match session.push_state(AgentState::Predicting { generation_id: 1 }) {
            Ok(_) => {}
            Err(_) => std::process::abort(),
        }
        let dto = SessionHeadDTO::from_session(&session);

        bencher.bench_local(|| {
            let blob_a = match serialize_head(&dto) {
                Ok(b) => b,
                Err(_) => std::process::abort(),
            };
            let blob_b = match serialize_head(&dto) {
                Ok(b) => b,
                Err(_) => std::process::abort(),
            };
            assert_eq!(blob_a, blob_b, "divergence between serializations");

            let dto_a = match deserialize_head(&blob_a) {
                Ok(d) => d,
                Err(_) => std::process::abort(),
            };
            let dto_b = match deserialize_head(&blob_b) {
                Ok(d) => d,
                Err(_) => std::process::abort(),
            };
            assert_eq!(dto_a, dto_b, "divergence between deserializations");
        });
    }

    /// Benchmark: `subroutine_cache_l1` — L1 lookup in `SubRoutineCache`.
    ///
    /// Target: < 1 µs for L1 lookup.
    ///
    /// Refs: I-Persist-Cache
    #[divan::bench]
    fn subroutine_cache_l1(bencher: divan::Bencher) {
        let mut cache = SubRoutineCache::new(NonZeroUsize::MIN.saturating_add(99));

        // Populate L2, then promote to L1.
        for i in 0..20usize {
            let dto = make_dto(&format!("sub-{i}"));
            cache.insert(format!("sub-{i}"), dto);
            cache.promote_to_l1(format!("sub-{i}"));
        }

        bencher.bench_local(|| {
            let result = cache.get("sub-10");
            assert!(result.is_some());
        });
    }

    /// Benchmark: `subroutine_cache_l2` — L2 lookup in `SubRoutineCache`.
    ///
    /// Measures the cost of an L2-only lookup (no L1 promotion).
    ///
    /// Refs: I-Persist-Cache
    #[divan::bench]
    fn subroutine_cache_l2(bencher: divan::Bencher) {
        let mut cache = SubRoutineCache::new(NonZeroUsize::MIN.saturating_add(99));

        // Populate L2 only.
        for i in 0..20usize {
            let dto = make_dto(&format!("sub-{i}"));
            cache.insert(format!("sub-{i}"), dto);
        }

        bencher.bench_local(|| {
            let result = cache.get("sub-10");
            assert!(result.is_some());
        });
    }

    /// Benchmark: `subroutine_cache_insert` — insertion cost with varying sizes.
    ///
    /// Refs: I-Persist-Cache
    #[divan::bench(args = [1usize, 5, 10, 20])]
    fn subroutine_cache_insert(bencher: divan::Bencher, count: usize) {
        let dtos: Vec<(String, SessionHeadDTO)> = (0..count)
            .map(|i| (format!("sub-{i}"), make_dto(&format!("sub-{i}"))))
            .collect();

        bencher
            .with_inputs(|| SubRoutineCache::new(NonZeroUsize::MIN.saturating_add(99)))
            .bench_local_refs(|cache| {
                for (id, dto) in &dtos {
                    cache.insert(id.clone(), dto.clone());
                }
            });
    }
}

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

fn main() {
    divan::main();
}
