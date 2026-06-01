//! Benchmark: `redb_idempotence` — Sprint 18.
//!
//! Verifies zero divergence between two serializations of the same session.
//! Measures save/load roundtrip time.
//!
//! Refs: I-Persist-Idempotence

use brioche_core::{AgentState, Session};
use brioche_shell_persistence::{SessionHeadDTO, deserialize_head, serialize_head};
use criterion::{Criterion, criterion_group, criterion_main};

fn bench_redb_idempotence(c: &mut Criterion) {
    let mut session = Session::new("idempotent");
    match session.push_state(AgentState::Predicting { generation_id: 1 }) {
        Ok(_) => {}
        Err(_) => std::process::abort(),
    }
    let dto = SessionHeadDTO::from_session(&session);

    c.bench_function("redb_idempotence", |b| {
        b.iter(|| {
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
    });
}

criterion_group!(benches, bench_redb_idempotence);
criterion_main!(benches);
