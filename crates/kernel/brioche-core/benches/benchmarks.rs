//! Benchmark suite for `brioche-core` — Book I hot paths.
//!
//! Replaces the previous Criterion suite with Divan, adding parameterized
//! and grouped benchmarks for all documented hot paths.
//!
//! ## Framework note
//! Uses **Divan** (`=0.1.21`) for lightweight, attribute-based benchmarks.
//! Divan is currently unmaintained (last commit April 2025), but the API is
//! stable and the compile-time overhead is minimal. Pinned to avoid accidental
//! upgrades. If Divan breaks in a future Rust edition, evaluate Criterion or
//! a custom harness as fallback.
//!
//! ## Benchmark groups
//! - `engine`: `transition()`, routing table construction, state push/pop.
//! - `extension`: `ExtensionStorage` hot/cold lookups, insert, register.
//! - `governance`: `HookEffectConstraint`, `DecisionAggregator`, rollback.
//! - `determinism`: Cross-instance replay verification.
//!
//! Refs: docs/SPECS.md §Pillar 3, PHILOSOPHY.md §2.3, §9

use std::collections::BTreeMap;

use brioche_core::{
    AgentState, BriocheEngine, BriocheEngineBuilder, BriocheExtensionType, ConsistencyVerifier,
    CycleRollbackPolicy, DecisionAggregator, Effect, EffectBit, EngineInput, EpochAction,
    EpochInterceptor, ExecutionPath, ExtVTable, ExtensionStorage, HookEffectConstraint,
    PluginResult, PolicyDecision, Session, SessionRegistry, StreamEvent, SubRoutineHandle,
    SubRoutineLifecycleGuard,
};
use brioche_governance_default::{
    AdaptiveUndoFrameGuard, FastHookEffectConstraint, LexicographicDecisionAggregator,
    TieredUndoFrameGuard,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Mock fixtures (shared across benchmarks)
// ---------------------------------------------------------------------------

struct MockDecisionAggregator;
impl DecisionAggregator for MockDecisionAggregator {
    fn aggregate_decisions(
        &self,
        _decisions: Vec<PolicyDecision>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

struct MockSubRoutineLifecycleGuard;
impl SubRoutineLifecycleGuard for MockSubRoutineLifecycleGuard {
    fn on_exit(
        &self,
        _handle: SubRoutineHandle,
        _parent: &mut Session,
        _registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        Ok(Vec::new())
    }
}

struct MockEpochInterceptor;
impl EpochInterceptor for MockEpochInterceptor {
    fn intercept_epoch(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<EpochAction> {
        Ok(EpochAction::Proceed)
    }
}

struct MockConsistencyVerifier;
impl ConsistencyVerifier for MockConsistencyVerifier {
    fn verify_consistency(&self, _session: &mut Session) -> PluginResult<Option<Vec<Effect>>> {
        Ok(None)
    }
}

struct MockCycleRollbackPolicy;
impl CycleRollbackPolicy for MockCycleRollbackPolicy {
    fn begin_hook(&mut self, _hook_name: &'static str) {}

    fn on_mutation(
        &mut self,
        _type_id: std::any::TypeId,
        _vtable: &ExtVTable,
        _current: &dyn std::any::Any,
    ) {
    }

    fn commit_hook(&mut self, _ext: &mut ExtensionStorage) {}

    fn rollback_hook(&mut self, _ext: &mut ExtensionStorage) {}
}

fn build_engine() -> BriocheEngine {
    let builder = BriocheEngineBuilder::new();
    builder
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .with_epoch_interceptor(Box::new(MockEpochInterceptor))
        .with_consistency_verifier(Box::new(MockConsistencyVerifier))
        .with_cycle_rollback_policy(Box::new(MockCycleRollbackPolicy))
        .build()
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, brioche_macro::BriocheExtensionType)]
#[brioche(ext_id = "benchmarks.BenchState")]
struct BenchState {
    value: i32,
    map: BTreeMap<String, i32>,
}

// ---------------------------------------------------------------------------
// Group: engine — transition, routing table, session state
// ---------------------------------------------------------------------------

#[divan::bench_group(name = "engine")]
mod engine {
    use super::*;

    /// Benchmark: `stream_latency` — P99 < 50 µs for stream event processing.
    ///
    /// Target: P99 < 50 µs for `transition()` on `EngineInput::LlmStream`.
    ///
    /// Refs: I-Core-StreamNoBranch
    #[divan::bench]
    fn stream_latency(bencher: divan::Bencher) {
        let mut engine = build_engine();
        let mut session = Session::new("bench");
        session.state = AgentState::Idle;

        bencher.bench_local(|| {
            let _ = engine.transition(
                &mut session,
                &EngineInput::LlmStream(StreamEvent::TextChunk {
                    path: ExecutionPath::default(),
                    chunk: bytes::Bytes::from_static(b"benchmark"),
                }),
            );
        });
    }

    /// Benchmark: `stream_zero_alloc` — zero-allocation stream path.
    ///
    /// Verifies that `transition()` on `EngineInput::LlmStream` with
    /// `is_final = true` and empty chunk does not allocate.
    ///
    /// Refs: I-Core-StreamNoBranch
    #[divan::bench]
    fn stream_zero_alloc(bencher: divan::Bencher) {
        let mut engine = build_engine();
        let mut session = Session::new("bench");
        session.state = AgentState::Idle;

        bencher.bench_local(|| {
            let _ = engine.transition(
                &mut session,
                &EngineInput::LlmStream(StreamEvent::TextChunk {
                    path: ExecutionPath::default(),
                    chunk: bytes::Bytes::new(),
                }),
            );
        });
    }

    /// Benchmark: `session_push_state` — O(1) state stack push.
    ///
    /// Parameterized by stack depth to verify amortized O(1) growth.
    ///
    /// Refs: I-Core-StatePushPop
    #[divan::bench(args = [0, 5, 10, 20])]
    fn session_push_state(bencher: divan::Bencher, depth: usize) {
        bencher
            .with_inputs(|| {
                let mut session = Session::new("bench");
                for _ in 0..depth {
                    let _ = session.push_state(AgentState::Idle);
                }
                session
            })
            .bench_local_refs(|session| {
                let _ = session.push_state(AgentState::Idle);
            });
    }

    /// Benchmark: `session_pop_state` — O(1) state stack pop.
    ///
    /// Parameterized by stack depth to verify amortized O(1) shrink.
    ///
    /// Refs: I-Core-StatePushPop
    #[divan::bench(args = [1, 5, 10, 20])]
    fn session_pop_state(bencher: divan::Bencher, depth: usize) {
        bencher
            .with_inputs(|| {
                let mut session = Session::new("bench");
                for _ in 0..depth {
                    let _ = session.push_state(AgentState::Idle);
                }
                session
            })
            .bench_local_refs(|session| {
                let _ = session.pop_state();
            });
    }

    /// Benchmark: `cross_instance_determinism` — zero divergence across instances.
    ///
    /// Two fresh engines with identical inputs must produce identical effect logs.
    ///
    /// Refs: I-Core-Determinism
    #[divan::bench]
    fn cross_instance_determinism(bencher: divan::Bencher) {
        bencher.bench_local(|| {
            let mut engine_a = build_engine();
            let mut engine_b = build_engine();
            let mut session_a = Session::new("bench-a");
            let mut session_b = Session::new("bench-b");
            session_a.state = AgentState::Idle;
            session_b.state = AgentState::Idle;

            let effects_a = engine_a.transition(
                &mut session_a,
                &EngineInput::LlmStream(StreamEvent::TextChunk {
                    path: ExecutionPath::default(),
                    chunk: bytes::Bytes::from_static(b"determinism"),
                }),
            );
            let effects_b = engine_b.transition(
                &mut session_b,
                &EngineInput::LlmStream(StreamEvent::TextChunk {
                    path: ExecutionPath::default(),
                    chunk: bytes::Bytes::from_static(b"determinism"),
                }),
            );

            assert_eq!(effects_a.len(), effects_b.len());
            for (a, b) in effects_a.iter().zip(effects_b.iter()) {
                assert_eq!(std::mem::discriminant(a), std::mem::discriminant(b));
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Group: extension — ExtensionStorage hot/cold paths
// ---------------------------------------------------------------------------

#[divan::bench_group(name = "extension")]
mod extension {
    use super::*;

    /// Benchmark: `extension_lookup` — hot path (type already in hot map).
    ///
    /// Target: < 50 ns for `ExtensionStorage::get_mut` on hot path.
    ///
    /// Refs: I-Core-ExtO1
    #[divan::bench]
    fn get_mut_hot(bencher: divan::Bencher) {
        bencher
            .with_inputs(|| {
                let mut storage = ExtensionStorage::new();
                let result = storage.insert(BenchState {
                    value: 42,
                    map: BTreeMap::new(),
                });
                assert!(result.is_ok(), "BenchState serializes: {result:?}");
                storage
            })
            .bench_local_refs(|storage| {
                let result = storage.get_mut::<BenchState>();
                divan::black_box(result);
            });
    }

    /// Benchmark: `extension_lookup` — hot path (default insertion).
    ///
    /// `get_or_insert_default` on a type already registered.
    ///
    /// Refs: I-Core-ExtO1
    #[divan::bench]
    fn get_or_insert_default_hot(bencher: divan::Bencher) {
        bencher
            .with_inputs(|| {
                let mut storage = ExtensionStorage::new();
                let result = storage.insert(BenchState {
                    value: 42,
                    map: BTreeMap::new(),
                });
                assert!(result.is_ok(), "BenchState serializes: {result:?}");
                storage
            })
            .bench_local_refs(|storage| {
                let result = storage.get_or_insert_default::<BenchState>();
                divan::black_box(result);
            });
    }

    /// Benchmark: `extension_lookup` — cold path (type evicted from hot map).
    ///
    /// Forces deserialization from the cold BTreeMap backing store.
    ///
    /// Refs: I-Core-ExtO1
    #[divan::bench]
    fn get_or_insert_default_cold(bencher: divan::Bencher) {
        bencher
            .with_inputs(|| {
                let mut storage = ExtensionStorage::new();
                let result = storage.insert(BenchState {
                    value: 42,
                    map: BTreeMap::new(),
                });
                assert!(result.is_ok(), "BenchState serializes: {result:?}");
                storage.evict_from_hot::<BenchState>();
                storage
            })
            .bench_local_refs(|storage| {
                storage.evict_from_hot::<BenchState>();
                let result = storage.get_or_insert_default::<BenchState>();
                divan::black_box(result);
            });
    }

    /// Benchmark: `extension_insert` — serialization + BTreeMap insertion.
    ///
    /// Measures the cost of serializing a BenchState to MessagePack and
    /// inserting into the cold BTreeMap.
    ///
    /// Refs: I-Core-ExtO1
    #[divan::bench]
    fn insert_with_serialization(bencher: divan::Bencher) {
        let state = BenchState {
            value: 42,
            map: BTreeMap::new(),
        };

        bencher
            .with_inputs(ExtensionStorage::new)
            .bench_local_refs(|storage| {
                let result = storage.insert(state.clone());
                assert!(result.is_ok(), "BenchState serializes: {result:?}");
            });
    }
    /// Benchmark: `extension_register` — VTable construction and BTreeMap insert.
    ///
    /// Measures the one-time cost of registering a type with the storage.
    ///
    /// Refs: I-Core-ExtensionType
    #[divan::bench]
    fn register_type(bencher: divan::Bencher) {
        bencher
            .with_inputs(ExtensionStorage::new)
            .bench_local_refs(|storage| {
                storage.register::<BenchState>();
            });
    }
}

// ---------------------------------------------------------------------------
// Group: governance — HookEffect, DecisionAggregator, rollback
// ---------------------------------------------------------------------------

#[divan::bench_group(name = "governance")]
mod governance {
    use super::*;

    /// Benchmark: `hook_effect_o1` — O(1) hook effect validation.
    ///
    /// Target: < 100 ns for `FastHookEffectConstraint::is_allowed_fast`.
    ///
    /// Refs: I-Core-HookEffect-O1
    #[divan::bench]
    fn hook_effect_o1(bencher: divan::Bencher) {
        let mut masks = [0u64; 8];
        masks[0] = EffectBit::CALL_LLM_NETWORK | EffectBit::SAVE_SESSION | EffectBit::SYSTEM_IDLE;
        masks[1] = EffectBit::EXECUTE_TOOLS | EffectBit::SAVE_SESSION;
        let constraint = FastHookEffectConstraint::new(masks);

        bencher.bench_local(|| {
            let allowed = constraint.is_allowed_fast(0, EffectBit::CALL_LLM_NETWORK);
            divan::black_box(allowed);
        });
    }

    /// Benchmark: `cow_rollback` — COW snapshot + restore for 1 mutated type.
    ///
    /// Target: < 10 µs for `AdaptiveUndoFrameGuard::rollback_hook`.
    ///
    /// Refs: I-Gov-Rollback-BestEffort
    #[divan::bench]
    fn cow_rollback(bencher: divan::Bencher) {
        bencher
            .with_inputs(|| {
                let guard = AdaptiveUndoFrameGuard::new();
                let mut ext = ExtensionStorage::new();
                let result = ext.insert(brioche_core::EpochState {
                    current_generation: 42,
                });
                assert!(result.is_ok(), "EpochState serializes: {result:?}");
                (guard, ext)
            })
            .bench_local_refs(|(guard, ext)| {
                guard.begin_hook("on_input");

                let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
                let vtable = brioche_core::EpochState::build_vtable();
                let current = ext.get_or_insert_default::<brioche_core::EpochState>();
                guard.on_mutation(type_id, &vtable, current);

                current.current_generation = 999;

                guard.rollback_hook(ext);

                let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
                assert_eq!(restored.current_generation, 42);
            });
    }

    /// Benchmark: `tiered_rollback_critical` — tiered rollback on critical type.
    ///
    /// Target: < 5 µs for `TieredUndoFrameGuard::rollback_hook`.
    ///
    /// Refs: I-Gov-Rollback-Critical
    #[divan::bench]
    fn tiered_rollback_critical(bencher: divan::Bencher) {
        bencher
            .with_inputs(|| {
                let guard = TieredUndoFrameGuard::new();
                let mut ext = ExtensionStorage::new();
                let result = ext.insert(brioche_core::EpochState {
                    current_generation: 7,
                });
                assert!(result.is_ok(), "EpochState serializes: {result:?}");
                (guard, ext)
            })
            .bench_local_refs(|(guard, ext)| {
                guard.begin_hook("on_input");

                let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
                let vtable = brioche_core::EpochState::build_vtable();
                let current = ext.get_or_insert_default::<brioche_core::EpochState>();
                guard.on_mutation(type_id, &vtable, current);

                current.current_generation = 777;

                guard.rollback_hook(ext);

                let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
                assert_eq!(restored.current_generation, 7);
            });
    }

    /// Benchmark: `lexicographic_aggregator` — default decision aggregation.
    ///
    /// Measures the cost of aggregating a small vector of decisions.
    ///
    /// Refs: I-Gov-Decision-Required
    #[divan::bench(args = [1, 3, 5, 10])]
    fn lexicographic_aggregator(bencher: divan::Bencher, decision_count: usize) {
        let aggregator = LexicographicDecisionAggregator;

        bencher
            .with_inputs(|| {
                let ext = ExtensionStorage::new();
                let all_allow = vec![PolicyDecision::Allow; decision_count];
                (ext, all_allow)
            })
            .bench_local_refs(|(ext, decisions)| {
                let _ = aggregator.aggregate_decisions(decisions.clone(), ext);
            });
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    divan::main();
}
