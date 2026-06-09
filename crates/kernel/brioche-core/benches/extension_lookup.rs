//! Criterion benchmark for `ExtensionStorage` hot-path lookups.
//!
//! Refs: I-Core-ExtO1

use std::collections::BTreeMap;
use std::hint::black_box;

use brioche_core::{BriocheExtensionType, ExtensionStorage};
use criterion::{Criterion, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};

/// Benchmark fixture state for extension storage.
#[derive(Clone, Default, Debug, Serialize, Deserialize, BriocheExtensionType)]
pub struct BenchState {
    /// Scalar field for weight measurement.
    pub value: u64,
    /// Ordered map field for weight measurement.
    pub map: BTreeMap<String, u64>,
}

fn bench_get_mut_hot(c: &mut Criterion) {
    let mut storage = ExtensionStorage::new();
    storage.insert(BenchState {
        value: 42,
        map: BTreeMap::new(),
    });

    c.bench_function("extension_get_mut_hot", |b| {
        b.iter(|| {
            let result = storage.get_mut::<BenchState>();
            black_box(result);
        });
    });
}

fn bench_get_or_insert_default_hot(c: &mut Criterion) {
    let mut storage = ExtensionStorage::new();
    storage.insert(BenchState {
        value: 42,
        map: BTreeMap::new(),
    });

    c.bench_function("extension_get_or_insert_default_hot", |b| {
        b.iter(|| {
            let result = storage.get_or_insert_default::<BenchState>();
            black_box(result);
        });
    });
}

fn bench_get_or_insert_default_cold(c: &mut Criterion) {
    let mut storage = ExtensionStorage::new();
    storage.insert(BenchState {
        value: 42,
        map: BTreeMap::new(),
    });
    storage.evict_from_hot::<BenchState>();

    c.bench_function("extension_get_or_insert_default_cold", |b| {
        b.iter(|| {
            storage.evict_from_hot::<BenchState>();
            let result = storage.get_or_insert_default::<BenchState>();
            black_box(result);
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
