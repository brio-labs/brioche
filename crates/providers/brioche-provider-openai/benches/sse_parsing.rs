//! Regression benchmarks for OpenAI SSE parsing.
//!
//! Measures parsing throughput for two shapes of input:
//!
//! 1. **Complete lines**: a single `Bytes` chunk containing many well-formed
//!    `data:` lines, including a trailing `[DONE]` line.
//! 2. **Fragment reconstruction**: the same logical stream split into small,
//!    arbitrary byte chunks so the parser must buffer and reassemble partial
//!    lines across calls.
//!
//! ## Determinism
//!
//! All payloads are generated from fixed templates; no network traffic is
//! involved.
//!
//! ## Budget
//!
//! No hard latency budget is recorded yet for SSE parsing; these benchmarks
//! are intended for regression detection. Per `CONTRIBUTING.md`, a regression
//! above 150% of the previous baseline blocks merge.
//!
//! Refs: I-Core-ChunkBudget, I-Shell-Network-Signal

#![allow(missing_docs)]

use brioche_provider_openai::sse::SseParser;
use bytes::Bytes;
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

/// Build a deterministic SSE payload containing `count` JSON data lines and
/// a trailing `data: [DONE]` line.
fn sse_lines(count: usize) -> Bytes {
    let mut payload = String::with_capacity(count * 64);
    for i in 0..count {
        payload.push_str(&format!(
            "data: {{\"choices\": [{{\"delta\": {{\"content\": \"{i}\"}}}}]}}\n"
        ));
    }
    payload.push_str("data: [DONE]\n");
    Bytes::from(payload)
}

/// Benchmark parsing throughput when every `data:` line arrives complete.
fn sse_complete_lines(c: &mut Criterion) {
    let mut group = c.benchmark_group("sse_complete_lines");

    for count in [100, 1_000, 10_000] {
        let input = sse_lines(count);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &input, |b, input| {
            b.iter(|| {
                let mut parser = SseParser::new();
                if let Ok(lines) = parser.feed(input) {
                    for line in lines {
                        black_box(line);
                    }
                }
            });
        });
    }

    group.finish();
}

/// Benchmark parsing throughput when the byte stream is fragmented.
///
/// The same 10 000-line payload is fed in chunks of varying size, forcing
/// the parser to retain partial lines in its internal buffer across calls.
fn sse_fragment_reconstruction(c: &mut Criterion) {
    let mut group = c.benchmark_group("sse_fragment_reconstruction");

    let total_lines = 10_000;
    let input = sse_lines(total_lines);

    for chunk_size in [7usize, 31, 128, 512] {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(chunk_size),
            &chunk_size,
            |b, &chunk_size| {
                b.iter(|| {
                    let mut parser = SseParser::new();
                    for chunk in input.chunks(chunk_size) {
                        if let Ok(lines) = parser.feed(&Bytes::copy_from_slice(chunk)) {
                            for line in lines {
                                black_box(line);
                            }
                        } else {
                            break;
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, sse_complete_lines, sse_fragment_reconstruction);
criterion_main!(benches);
