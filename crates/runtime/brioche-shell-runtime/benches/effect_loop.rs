//! Regression benchmarks for the shell effect-consumption loop.
//!
//! Measures how fast a stream of [`Effect`] values emitted by the kernel can
//! be dispatched by the async shell runtime. The fixture uses a minimal
//! governance profile and a plugin that converts every [`EngineInput`] into a
//! deterministic batch of [`Effect::ForwardToUi`] effects. A UI forwarder
//! callback counts completed effects so the benchmark can wait for the full
//! batch without relying on wall-clock timeouts.
//!
//! ## Determinism
//!
//! Inputs, plugin behavior, and effect counts are fixed. No network or
//! external I/O is exercised.
//!
//! ## Budget
//!
//! No hard latency budget is recorded yet for the effect loop; these
//! benchmarks are intended for regression detection. Per `CONTRIBUTING.md`,
//! a regression above 150% of the previous baseline blocks merge.
//!
//! Refs: I-Shell-Runtime-OnlyIO, I-Shell-Backpressure-NoOverflow

#![allow(missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use brioche_core::{
    BriocheEngineBuilder, BriochePlugin, Effect, EngineInput, ExtensionStorage, PluginCapabilities,
    PluginResult, PolicyDecision, Session, UiWidget,
};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence,
    ShellConfig,
};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

/// Plugin that short-circuits every input into a fixed-size effect batch.
///
/// By returning [`PolicyDecision::OverrideTransition`] from `on_input`, the
/// kernel bypasses prediction and tool phases and emits the requested effects
/// directly into the shell effect loop.
struct EffectPumpPlugin {
    /// Number of [`Effect::ForwardToUi`] effects produced per input.
    effects_per_input: usize,
}

impl BriochePlugin for EffectPumpPlugin {
    fn name(&self) -> &'static str {
        "effect-pump"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        // Run before any standard plugin so the override always wins.
        i16::MIN
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let effects: Vec<Effect> = (0..self.effects_per_input)
            .map(|i| Effect::ForwardToUi(UiWidget::Status(format!("effect-{i}"))))
            .collect();
        Ok(PolicyDecision::OverrideTransition(effects))
    }
}

/// Create a Tokio runtime for Criterion's async executor.
fn tokio_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap_or_else(|err| {
        eprintln!("failed to create Tokio runtime: {err}");
        std::process::exit(1);
    })
}

/// Build a shell wired to emit `effects_per_input` UI effects for every input.
fn make_shell_with_executor<E>(
    effects_per_input: usize,
    engine_channel_capacity: usize,
    executor: E,
) -> BriocheShell
where
    E: brioche_shell_runtime::EffectExecutor + 'static,
{
    BriocheShell::new(
        move || {
            let engine = BriocheEngineBuilder::new()
                .with_profile(GovernanceProfile::Permissive)
                .with_plugin(Box::new(EffectPumpPlugin { effects_per_input }))
                .with_default_tool_timeout_ms(1_000)
                .build();
            let session = Session::new("bench");
            (engine, session)
        },
        ShellConfig {
            engine_channel_capacity,
            tick_interval_ms: u64::MAX,
            transition_journal_enabled: false,
            ..ShellConfig::default()
        },
        executor,
        None,
    )
}

/// Build an executor whose UI forwarder increments `counter` for every effect.
fn counting_executor(
    counter: &Arc<AtomicU64>,
) -> DefaultEffectExecutor<EchoToolExecutor, MockLlmClient, NoopPersistence> {
    let counter = Arc::clone(counter);
    DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence)
        .with_ui_forwarder(move |_widget: UiWidget| {
            counter.fetch_add(1, Ordering::Release);
        })
}

/// Benchmark effect-loop throughput for a single large batch of effects.
fn effect_loop_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("effect_loop_throughput");
    let runtime = tokio_runtime();

    for &effects in &[100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(effects as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(effects),
            &effects,
            |b, &effects| {
                b.to_async(&runtime).iter(|| async move {
                    let counter = Arc::new(AtomicU64::new(0));
                    let executor = counting_executor(&counter);
                    let shell = make_shell_with_executor(effects, 256, executor);
                    let total = effects as u64;

                    let _ = shell
                        .send_input(EngineInput::UserMessage("go".into()))
                        .await;

                    while counter.load(Ordering::Acquire) < total {
                        tokio::task::yield_now().await;
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark effect-loop throughput under strict backpressure.
///
/// The engine input channel is intentionally shrunk to a capacity of 8 so
/// producers block once the channel fills, exercising the bounded-channel
/// backpressure path.
fn effect_loop_backpressure(c: &mut Criterion) {
    let mut group = c.benchmark_group("effect_loop_backpressure");
    let runtime = tokio_runtime();

    const TOTAL_EFFECTS: u64 = 10_000;
    const INPUTS: usize = 1_000;
    const EFFECTS_PER_INPUT: usize = (TOTAL_EFFECTS as usize) / INPUTS;

    group.throughput(Throughput::Elements(TOTAL_EFFECTS));
    group.bench_function("strict_capacity_8", |b| {
        b.to_async(&runtime).iter(|| async move {
            let counter = Arc::new(AtomicU64::new(0));
            let executor = counting_executor(&counter);
            let shell = make_shell_with_executor(EFFECTS_PER_INPUT, 8, executor);

            for _ in 0..INPUTS {
                if shell
                    .send_input(EngineInput::UserMessage("go".into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }

            while counter.load(Ordering::Acquire) < TOTAL_EFFECTS {
                tokio::task::yield_now().await;
            }
        });
    });

    group.finish();
}

criterion_group!(benches, effect_loop_throughput, effect_loop_backpressure);
criterion_main!(benches);
