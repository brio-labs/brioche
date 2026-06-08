use brioche_core::{BriocheExtensionType, ExtensionStorage};
use criterion::{Criterion, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hint::black_box;

#[derive(Clone, Default, Debug, Serialize, Deserialize, BriocheExtensionType)]
pub struct BenchState {
    pub value: u64,
    pub map: BTreeMap<String, u64>,
}

fn bench_get_mut_hot(c: &mut Criterion) {
    let mut storage = ExtensionStorage::new();
    assert!(
        storage
            .insert(BenchState {
                value: 42,
                map: BTreeMap::new(),
            })
            .is_ok()
    );

    c.bench_function("extension_get_mut_hot", |b| {
        b.iter(|| {
            let result = storage.get_mut::<BenchState>();
            black_box(result);
        });
    });
}

fn bench_get_or_insert_default_hot(c: &mut Criterion) {
    let mut storage = ExtensionStorage::new();
    assert!(
        storage
            .insert(BenchState {
                value: 42,
                map: BTreeMap::new(),
            })
            .is_ok()
    );

    c.bench_function("extension_get_or_insert_default_hot", |b| {
        b.iter(|| {
            storage.with_or_insert_default::<BenchState, _>(|result| {
                black_box(result);
            });
        });
    });
}

fn bench_get_or_insert_default_cold(c: &mut Criterion) {
    let mut storage = ExtensionStorage::new();
    assert!(
        storage
            .insert(BenchState {
                value: 42,
                map: BTreeMap::new(),
            })
            .is_ok()
    );
    storage.evict_from_hot::<BenchState>();

    c.bench_function("extension_get_or_insert_default_cold", |b| {
        b.iter(|| {
            storage.evict_from_hot::<BenchState>();
            storage.with_or_insert_default::<BenchState, _>(|result| {
                black_box(result);
            });
        });
    });
}

criterion_group!(
    benches,
    bench_get_mut_hot,
    bench_get_or_insert_default_hot,
    bench_get_or_insert_default_cold
);
criterion_main!(benches);
