# Brioche Development Roadmap

> Living document tracking development progress through the 18-sprint plan.
> Updated incrementally as each step is completed.

---

## Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Complete — code merged, tests passing, spec updated |
| 🔄 | In Progress — branch open, partial implementation |
| ⏳ | Planned — spec ready, not yet started |
| ❌ | Removed — scope cut, documented rationale |

---

## Phase 0: Foundation (Sprint 0)

### Sprint 0: Repo Setup ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| Workspace layout with 11 crates | ✅ | `crates/*` defined in `Cargo.toml` |
| CI/CD pipeline (`ci.yml`) | ✅ | deny, fmt, clippy, test, test-doc, docs, audit |
| Conventional Commits enforcement | ✅ | PR title + commit message validation |
| Branch protection templates | ✅ | `CODEOWNERS`, PR template, issue templates |
| GPG setup script | ✅ | `scripts/setup-gpg.sh` |
| Pre-commit hook | ✅ | `scripts/pre-commit.sh` |
| Invariant lint script | ✅ | `scripts/check-invariants.sh` |
| `deny.toml`, `clippy.toml`, `rustfmt.toml` | ✅ | Enforced in CI |

**Exit Criteria:** `ci.yml` passes on empty workspace; signed commits mandatory; labels/templates active — **ACHIEVED**

---

## Phase 1: The Kernel — Book I (Sprints 1–5)

### Sprint 1: Book I Spec Skeleton + `brioche-macro` Scaffold ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `docs/architecture/book-i-core.md` (Ch 1-3) | ✅ | Spec skeleton for Book I (Ch 1-3) |
| `BriocheExtensionType` proc-macro | ✅ | Compile-time verification macro |
| `trybuild` tests for macro | ✅ | 1 pass + 5 compile-fail (`HashMap`, `HashSet`, missing `Clone`, UI type, manual impl) |
| `cargo test -p brioche-macro` passes | ✅ | |
| `EXT_ID` uniqueness enforced | ✅ | Auto-generated via `concat!(module_path!(), "::", stringify!(T))` |
| Sealed trait pattern | ✅ | `__private::Sealed` supertrait blocks manual impls |

**Key Invariants Targeted:** I-Core-ExtensionType, I-Core-VTableClone — **ACHIEVED**

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo test -p brioche-macro` ✅ (6 trybuild cases)

---

### Sprint 2: ExtensionStorage + ExtVTable ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `ExtensionStorage` hot_map / cold_snapshot / registry | ✅ | O(1) access by `TypeId` |
| `ExtVTable` with serialize, deserialize, clone_box, estimated_weight_bytes | ✅ | Uses `postcard` for binary serialization |
| `get_mut` < 50 ns target | ✅ | ~11.4 ns (criterion, P50) |
| `get_or_insert_default` infallible | ✅ | `Box::leak` fallback for borrow-checker safety |

**Key Invariants Targeted:** I-Core-ExtO1, I-Core-VTableClone — **ACHIEVED**

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo deny check all` ✅
- Criterion benchmarks: `extension_get_mut_hot` P50 = ~11–18 ns ✅ (target: < 50 ns)
- Property tests (`proptest`) for `ExtensionStorage` roundtrip + infallibility ✅
- `trybuild` tests: 1 pass + 7 compile-fail ✅ (added `fail_vec_undetermined`)

---

### Sprint 3: Session, AgentState, SessionRegistry, SessionSnapshot ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `Session` with `!Send` / `!Sync` | ✅ | `PhantomData<*mut ()>` marker on stable Rust |
| `AgentState` enum (Idle, Predicting, ExecutingTools, SubRoutine, Failure) | ✅ | Pure mechanical states only |
| `SessionRegistry` with `!Send` / `!Sync` | ✅ | `BTreeMap<SubRoutineHandle, Session>` + exit_counts |
| `SessionSnapshot` as `BriocheExtensionType` | ✅ | Injected into `ExtensionStorage` before each hook |
| `EngineInput`, `Effect`, `PolicyDecision` enums | ✅ | Deterministic, serializable, no hidden side effects |
| `seal()` function: `ToolCallDescriptor` → `ActiveToolCall` | ✅ | Exhaustive match; `None` timeout defaults to `0` |
| `PluginError`, `BriocheError`, `PluginResult<T>` | ✅ | `thiserror` derives; `#[non_exhaustive]` on `BriocheError` |
| Integration tests for stack ops, registry, seal, snapshot | ✅ | 15 tests in `tests/session_types.rs` |

**Key Invariants Targeted:** I-Core-Pure, I-Core-NoPanic — **ACHIEVED**

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo deny check all` ✅
- `trybuild` tests updated: 1 pass + 7 compile-fail ✅

---

### Sprint 4: `BriocheEngine::transition()` + `UnifiedRoutingTable` ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `BriochePlugin` trait + `PluginCapabilities` bitmask | ✅ | 7 hook points with default implementations |
| `UnifiedRoutingTable` | ✅ | O(1) index pre-routing; sorted by `(priority, name)` |
| `BriocheEngine` + `BriocheEngineBuilder` | ✅ | Mandatory trait enforcement at build time |
| `transition()` main algorithm (13 steps) | ✅ | EpochInterceptor → SubRoutineHandler → on_input → dispatch → lifecycle → consistency |
| Governance trait definitions | ✅ | 10 traits: EpochInterceptor, SubRoutineHandler, ConsistencyVerifier, DecisionAggregator, SignalDrainOrder, HookEffectConstraint, CycleRollbackPolicy, SubRoutineLifecycleGuard, GovernanceFailoverHandler, CowBudgetPolicy |
| `EffectBit` + `effect_to_bitmask()` | ✅ | O(1) hook effect validation infrastructure |
| `EpochAction`, `EpochState`, `TransitionTraceLog`, `SupersededTransitionTraceLog` | ✅ | Critical-state extension types |
| `HistoryEdit` application with index validation | ✅ | Insert, Replace, Truncate — infallible inside engine |
| `RebuildRoutes` last-position guarantee | ✅ | Truncates anything after `RebuildRoutes` |
| Integration tests: 18 cases | ✅ | `tests/engine_transition.rs` |

**Key Invariants Targeted:** I-Core-StreamNoBranch, I-Core-PluginOrder, I-Core-NoPanic, I-Core-RetVecEffect — **ACHIEVED**

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo deny check all` ✅
- `trybuild` tests updated: 1 pass + 7 compile-fail ✅

---

### Sprint 5: `EngineInput` dispatch refinement, `seal()` integration, `ActiveToolCall` materialization ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `EngineInput` dispatch refinement for `LlmStream` tool calls | ✅ | `ToolCallStart` / `ToolArgumentChunk` / `ToolCallDone` accumulation |
| `seal()` integration in `dispatch_llm_stream` | ✅ | `materialize_tool_calls()` applies `default_tool_timeout_ms` + emits `Effect::Error` on missing timeout |
| `ActiveToolCall` materialization | ✅ | Stored in `session.active_tools` after `on_tool_calls` hook + `seal()` |
| `StreamToolAccumulator` transient type | ✅ | `#[brioche(no_snapshot)]` — no COW rollback needed |
| `handle_tool_calls()` helper | ✅ | Pre-routed `on_tool_calls` hook invocation |
| `BriocheEngineBuilder::with_default_tool_timeout_ms()` | ✅ | Configurable safeguard for missing timeouts |
| Integration tests: 3 new cases | ✅ | `tool_call_materialization`, `missing_timeout_applies_default`, `on_tool_calls_mutates_timeout` |

**Key Invariants Targeted:** I-Core-RetVecEffect, I-Core-ChunkBudget, I-Core-ActiveToolCall — **ACHIEVED**

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅ (21 engine tests)
- `trybuild` tests unchanged: 1 pass + 7 compile-fail ✅

---

## Phase 2: Governance — Book II (Sprints 6–8)

### Sprint 6: Fundamental Traits ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `EpochGuard` (`EpochInterceptor`) | ✅ | Silent rejection of stale-epoch inputs |
| `LexicographicDecisionAggregator` (`DecisionAggregator`) | ✅ | Deterministic fusion rule: Block > Override > Mutate > Request > Allow |
| `SubRoutineCleanupGuard` (`SubRoutineLifecycleGuard`) | ✅ | `SessionRegistry` cleanup + `SaveSession` |
| `StateConsistencyGuard` (`ConsistencyVerifier`) | ✅ | Force to `Idle` if active state without stack |
| `FastHookEffectConstraint` (`HookEffectConstraint`) | ✅ | O(1) binary mask `u64` validation |
| `NoopCycleRollbackPolicy` (`CycleRollbackPolicy`) | ✅ | Null impl; mechanical COW deferred to Sprint 7 |
| `SystemFailoverGuard` (`GovernanceFailoverHandler`) | ✅ | `ForwardToUi(critical_error)` + `SystemIdle` on fault |
| `SubRoutineOrchestrator` (`SubRoutineHandler`) | ✅ | Basic sub-routine delegation (Sprint 7+ for `transition(child)`) |
| ~~`CycleBudgetGuard` (`CycleBudgetPolicy`)~~ | ❌ **Removed** | Time-based per-hook budget moved to Shell Runtime (`EngineWatchdog`). Core remains deterministic; `Instant::now()` forbidden by PHILOSOPHY.md §2.2. |
| `docs/architecture/book-ii-governance.md` | ✅ | Spec Book II (Ch 1–4) |
| Integration tests: 7 new cases | ✅ | `tests/engine_transition.rs` |

**Key Invariants Targeted:** I-Gov-Epoch-Reject, I-Gov-Traits-Order, I-Gov-Decision-Required — **ACHIEVED**

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅ (28 engine tests)
- `cargo deny check all` ✅
- `cargo fmt` ✅

---

### Sprint 7: Optional Traits + COW Integration ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `CycleRollbackPolicy` trait → `&mut self` | ✅ | Required for frame state storage in `UndoFrameGuard` |
| `ExtensionStorage` COW observer (`set_cow_observer` / `clear_cow_observer`) | ✅ | `on_mutation` notification at first `get_mut` per hook |
| `ExtensionStorage::restore_boxed` | ✅ | Hot_map restoration from COW snapshot |
| `UndoFrameGuard` (`CycleRollbackPolicy`) | ✅ | Granular COW snapshot; 64 KB default threshold; `CriticalFullClone` exempt |
| `ToolExecutionTracker` (`BriochePlugin`) | ✅ | Telemetry `completed_count` / `failed_count` via `on_tool_calls` + `on_tool_result` |
| `PluginCapabilities::bitor` (`BitOr`) | ✅ | Capability composition (e.g. `ON_TOOL_CALLS | ON_TOOL_RESULT`) |
| Engine hook instrumentation (`with_rollback`) | ✅ | `begin_hook` → observer → hook → `commit_hook` / `rollback_hook` + `PluginFault(Soft)` if budget exceeded |
| Integration tests: 4 new cases | ✅ | `undo_frame_guard_restores_mutated_extension`, `undo_frame_guard_abandons_past_threshold`, `tool_execution_tracker_counts_outcomes`, `engine_with_undo_frame_guard_instruments_hooks` |

**Key Invariants Targeted:** I-Gov-Rollback-BestEffort, I-Gov-Rollback-Critical, I-Core-StreamNoBranch — **ACHIEVED**

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅ (32 engine tests)
- `cargo deny check all` ✅
- `cargo fmt` ✅

---

### Sprint 8: `brioche-governance-default` + Remaining Governance Plugins ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `brioche-governance-default` crate scaffold | ✅ | Expanded with 15 new modules |
| `GovernanceProfile` enum (Permissive / Standard / Strict) | ✅ | `profile.rs` with `apply()` and `BriocheEngineBuilderExt` |
| `BriocheEngineBuilder::with_profile()` | ✅ | Extension trait `BriocheEngineBuilderExt` in governance-default |
| `QuarantineManager` (`BriochePlugin`) | ✅ | `on_error` hook → `RebuildRoutes` on fatal |
| `RecoveryPolicy` (`BriochePlugin`) | ✅ | `RecoveryState` structure for shell-side signal drainage |
| `DepthGuard` (`BriochePlugin`) | ✅ | `DepthState` + `OverrideTransition` on limit exceeded |
| `TransitionConflictLogger` (`BriochePlugin`) | ✅ | `after_prediction` observer of `SupersededTransitionTraceLog` |
| `ToolCallDetector` (`BriochePlugin`) | ✅ | `on_stream_event` counts `ToolCallStart` / `ToolCallDone` |
| `JsonArgumentAccumulator` (`BriochePlugin`) | ✅ | `on_stream_event` buffers argument fragments |
| `ToolResultFormatter` (`BriochePlugin`) | ✅ | `on_tool_result` truncates oversized results |
| `ToolTimeoutPolicy` (`BriochePlugin`) | ✅ | `on_tool_calls` applies default + max bounds |
| `SubRoutineTimeoutPolicy` (`BriochePlugin`) | ✅ | `SubRoutineTimerState` for tick-based timeout (shell-side) |
| `AdaptiveUndoFrameGuard` (`CycleRollbackPolicy`) | ✅ | Consults `CowBudgetPolicy`; 64 KB fallback |
| `TieredUndoFrameGuard` (`CycleRollbackPolicy`) | ✅ | Critical / Standard / BestEffort tiers |
| `HistoricalCowBudgetPolicy` (`CowBudgetPolicy`) | ✅ | Sliding-window success-rate auto-tuning |
| `NegotiationBroker` (`DecisionAggregator`) | ✅ | Up to 3 phases; settles on consensus or timeout |
| `TreeDecisionAggregator` (`DecisionAggregator`) | ✅ | `DecisionNode` / `DecisionCondition` tree in `ExtensionStorage` |
| `RollbackTelemetryEmitter` (`BriochePlugin`) | ✅ | `after_prediction` passive observer |
| `PermissiveHookEffectConstraint` (`HookEffectConstraint`) | ✅ | `u64::MAX` masks on all hooks |
| `Noop*` reference implementations | ✅ | `NoopGovernanceFailoverHandler`, `NoopHookEffectConstraint`, `NoopCowBudgetPolicy`, `NoopCycleRollbackPolicy` |
| `GovernanceCompatibilityMatrix` | ✅ | `CompatibilityLevel` entries + symmetric lookup |
| Integration tests: governance profile matrix | ✅ | 15 tests in `tests/governance_profiles.rs` |

**Key Invariants Targeted:** I-Gov-SubRoutineLifecycle-Guard, I-Gov-Profile-Agnostic, I-Gov-Failover-LastResort, I-Gov-CowBudget-Adaptative, I-Gov-Tiered-Rollback, I-Comp-Override-Rebuild, I-Comp-Epoch-First

**Exit Criteria:** All three governance profiles boot a `BriocheEngine` in under 5 lines; `cargo test -p brioche-governance-default` passes; benchmark regression < 150%. — **ACHIEVED**

---

## Phase 3: Shell Runtime — Book III-A (Sprints 9–11)

### Sprint 9: Event Loop, Effect Execution, BackpressureRegulator ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `brioche-shell-runtime` crate scaffold | ✅ | Tokio-based async runtime |
| `EngineInput` mpsc channel + dispatch loop | ✅ | Unidirectional channel to kernel |
| Effect consumption loop | ✅ | Async task spawning per `Effect` variant |
| `ExecuteTools` parallel execution with `CancellationToken` | ✅ | `tokio::select!` timeout + cancel branches |
| `CallLlmNetwork` SSE streaming with `MAX_INLINE_CHUNK` segmentation | ✅ | Zero-copy `Bytes` fragmentation |
| `ExecuteCpuTask` via `tokio::task::spawn_blocking` | ✅ | Offload CPU work without blocking engine |
| `SaveSession` / `SavePluginBlob` async execution | ✅ | Delegates to `Persistence` trait (noop placeholder) |
| `TriggerSummarization` background LLM call | ✅ | Emits `AsyncTaskResult::SummarizationDone` |
| `TriggerGc` with `CancellationToken` | ✅ | Interruptible GC task (placeholder) |
| `BackpressureRegulator` | ✅ | Bounded channel, drop policy for text chunks under pressure |
| `SystemIdle` handling + `GcPolicy` trigger | ✅ | Post-idle hook ready for Sprint 16 `GcPolicy` |
| `SystemSignal`, `AsyncTaskResult`, `GovernanceNotification` types in core | ✅ | Added to `brioche-core/src/types.rs` |
| Integration tests | ✅ | 7 tests in `tests/shell_runtime.rs` |

**Key Invariants Targeted:** I-Shell-ToolResult-PassThrough, I-Shell-Backpressure-NoOverflow, I-Shell-Tick, I-Shell-Network-Signal

---

### Sprint 10: SignalMultiplexer, UnifiedEventBus, EngineWatchdog ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `SystemSignalAdapter` (SPSC queue) | ✅ | `drain()` consumes receiver into `Vec<SystemSignal>` |
| `AsyncTaskResultAdapter` (SPSC queue) | ✅ | `drain()` consumes receiver into `Vec<AsyncTaskResult>` |
| `GovernanceNotificationAdapter` (SPSC queue) | ✅ | `drain()` consumes receiver into `Vec<GovernanceNotification>` |
| `SignalMultiplexer` (`SignalDrainOrder`) | ✅ | Canonical order: SystemSignal > GovernanceNotification > AsyncTaskResult |
| `UnifiedEventBus` (optional) | ✅ | `EngineEnvelope` flow with fast-path bypass |
| `EngineWatchdog` bi-directional heartbeat | ✅ | Ping-pong with `last_epoch` + `pending_inputs` |
| `EngineWatchdog` recovery procedure | ✅ | `SerializeAndRestart` / `NotifyAndDegrade` placeholders |
| Periodic `SystemSignal::Tick` emitter | ✅ | Wired into `BriocheShell::new`, default 1000 ms |
| Telemetry channel + non-blocking subscriber | ✅ | `TelemetryChannel` with broadcast + default tracing subscriber |
| `SignalBuffer` + `SignalDrainBatch` in core | ✅ | Transient `ExtensionStorage` type for inter-cycle signal injection |
| Integration tests | ✅ | 8 new tests in `tests/shell_runtime.rs` (15 total) |

**Key Invariants Targeted:** I-Shell-Drain-Atomic, I-Shell-Watchdog-NoKill, I-Shell-Watchdog-Recovery, I-Shell-Session-NoSend

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo fmt --check` ✅
- `cargo deny check all` ✅
- `trybuild` tests updated for `SignalBuffer` in `__private::Sealed` list ✅

---

### Sprint 11: TransitionJournal, PersistenceMode, Shell Runtime Integration ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `TransitionJournal` (lock-free shared memory) | ✅ | 1 MB pre-allocated, persists each `EngineInput` before `transition()` |
| `PersistenceMode` (`Async` / `Sync`) | ✅ | Controls `SaveSession` flush behavior |
| `NetworkRecovery` shell plugin | ✅ | Retry/backoff at transport level; `SystemSignal::NetworkUnavailable` as last resort |
| Shell startup procedure (9 steps) | ✅ | See SPECS.md Book III-A §1.1 |
| `PluginFault` handling: `GovernanceChannel` → `QuarantineManager` | ✅ | End-to-end fault propagation |
| `RebuildRoutes` transactional barrier | ✅ | O(N) route recalc without engine restart |
| Integration tests: shell runtime | ✅ | 21 async tests (6 new for Sprint 11) |

**Key Invariants Targeted:** I-Shell-TransitionJournal, I-Shell-Persistence-Mode, I-Gov-Rebuild-Barrier

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo fmt --check` ✅
- `cargo deny check all` ✅

---

## Phase 4: Shell Persistence — Book III-B (Sprints 12–13)

### Sprint 12: Redb Schema, SubRoutineCache, Save Protocol ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `brioche-shell-persistence` crate scaffold | ✅ | Redb, `rmp-serde`, `zstd`, `lru` dependencies |
| `SESSIONS_TABLE` (MessagePack DTO) | ✅ | Session head: state, extensions, flattened stack |
| `MESSAGES_TABLE` (append-only, composite key) | ✅ | `(session_id, message_index)`; Zstd if > 1 KB |
| `SessionHeadDTO` with versioned schema | ✅ | `V1` / `V2` enum; Read-Upgrade-Write migration |
| Delta save protocol | ✅ | Extract messages from `persisted_msg_count` to end |
| Flattening protocol (`SubRoutine` → child ID) | ✅ | Opaque handle storage for serialization |
| `SubRoutineCache` L1 Visible / L2 LRU | ✅ | L1 = UI-open accordions (never evicted); L2 = LRU |
| `SavePluginBlob` async cold blob write | ✅ | `spawn_blocking` without engine blocking |

**Key Invariants Targeted:** I-Persist-SaveSession, I-Persist-AppendOnly, I-Persist-PluginBlob, I-Persist-Cache, I-Shell-SubRoutineCache

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅ (17 persistence tests)
- `cargo fmt --check` ✅
- `cargo deny check all` ✅

---

### Sprint 13: Loading, Rehydration, Opportunistic GC ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| Lazy session loading (head only, depth-1 children batch) | ✅ | `LazySessionLoader` pre-fills `SubRoutineCache.l2_lru` |
| On-demand sub-routine loading (`load_subroutine` IPC) | ✅ | `load_subroutine`: L1 → L2 → Redb → `RestoreSubRoutine` |
| `ExtensionStorage::hydrate_plugin` individual recovery | ✅ | Corrupted blob → default via VTable fallback |
| Opportunistic GC trigger + execution | ✅ | `GcRunner::run_gc` removes messages below `compaction_index` |
| GC `CancellationToken` premature interrupt | ✅ | `GcRunner::cancel()` breaks scan, commits partial progress |
| Idempotence verification tests | ✅ | Two serializations → bit-for-bit identical MessagePack |
| Integration tests: persistence roundtrip | ✅ | Save → load → replay → identical effects |

**Key Invariants Targeted:** I-Persist-GC-Interrupt, I-Persist-Idempotence, I-Shell-Load-Batch

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅ (25 persistence tests + 11 extension tests)
- `cargo deny check all` ✅
- `cargo fmt` ✅

---

## Phase 5: Shell Projection — Book III-C (Sprints 14–15)

### Sprint 14: UiRegistry, ContentRenderer, UiComposer ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `brioche-shell-projection` crate scaffold | ✅ | Vue 3 + Tauri IPC |
| `UiRegistry` with anchor slots | ✅ | `top-bar`, `sidebar`, `status-bar`, `input-actions`, `input-overlay`, `content-renderer`, `message-footer`, `settings-panel` |
| `ContentRenderer` streaming engine | ✅ | `StreamBuffer` with `shallowRef` + `requestAnimationFrame` |
| `UiComposer` per-frame budget | ✅ | 2 ms default; priority tiers: TextChunk > Navigation > Semantic > Cosmetic |
| Special governance widgets | ✅ | `system_degraded`, `network_error`, `status`, `error`, `subroutine_timeout` |
| `UiPerformancePolicy` plugin | ✅ | Configures `UiComposer` frame budget via `ExtensionStorage` |

**Key Invariants Targeted:** I-UI-NoUIType, I-UI-NoDirectDOM, I-UI-StreamBuffer, I-UI-Composer-FrameSync

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅ (37 projection tests)
- `cargo deny check all` ✅
- `cargo fmt --check` ✅
- `docs/architecture/book-iii-c-projection.md` added ✅

---

### Sprint 15: Tauri IPC, Sub-routine Management UI ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| Tauri `send_message` IPC command | ✅ | `IpcCommandService::send_message` injects `EngineInput::UserMessage` |
| Tauri `cancel_action` IPC command | ✅ | `IpcCommandService::cancel_action` emits `SystemSignal::OperationCancelled` |
| Tauri `load_subroutine` IPC command | ✅ | `IpcCommandService::load_subroutine` checks `SubRoutineCache` (L1→L2→Redb) then sends `RestoreSubRoutine` |
| `stream_batch` event channel (MessagePack) | ✅ | `StreamBatch` + `StreamBatchEmitter` with `rmp_serde` serialization |
| Sub-routine accordion states (`idle` → `loading` → `loaded` → `error` → `timeout`) | ✅ | `SubRoutineAccordionState` enum + `SubRoutineManager` lifecycle |
| Isolated `ContentRenderer` per sub-routine | ✅ | `SubRoutineUiState` holds its own `ContentRenderer`; `BTreeMap` indexed by `SubRoutineHandle` |
| IPC rate limiting (< 1 event/frame) | ✅ | `IpcRateLimiter` with frame budget + lock-free CAS timestamp |
| Integration tests: Tauri end-to-end | ✅ | 20 lib tests + 42 integration tests (5 new Sprint 15 tests) |

**Key Invariants Targeted:** I-UI-IPC-Rate, I-Shell-NoUIType, I-UI-StreamBuffer, I-Eco-OrderedCollections

---

## Phase 6: Ecosystem — Book IV (Sprints 16–17)

### Sprint 16: Standard Plugins ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `CircuitBreaker` (`before_prediction`) | ✅ | Detects redundant tool call loops |
| `TokenTracker` (`before_prediction` + `after_prediction`) | ✅ | Real-time cost/volume tracking |
| `ContextOptimizer` (`before_prediction`) | ✅ | Triggers `TriggerSummarization` at 85% threshold |
| `ToolTimeoutPolicy` (`on_tool_calls`) | ✅ | Re-export from governance-default with std defaults |
| `ToolResultPolicy` (`on_tool_result`) | ✅ | Truncates oversized results |
| `PendingTaskManager` (`on_tool_result` + `AsyncTaskResult`) | ✅ | Long-task `Pending` pattern |
| `GcPolicy` (`after_prediction`) | ✅ | Decides `TriggerGc` on `SystemIdle` |
| `AuditLogger` (`on_input`) | ✅ | Deterministic replay log with batching |
| `brioche-std` crate integration | ✅ | All standard plugins exported from `brioche_std` |
| Integration tests: standard plugin matrix | ✅ | 19 tests in `tests/standard_plugins.rs` |

**Key Invariants Targeted:** I-Eco-ExtensionOverMod, I-Eco-NoDirectMutation, I-Eco-OrderedCollections, I-Eco-Decision-Isolation

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo fmt --check` ✅
- `cargo deny check all` ✅

---

### Sprint 17: Plugin Kit, Playground, Developer Tooling ✅

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `brioche-plugin-kit` crate | ✅ | `#[brioche_plugin]`, `#[hook(...)]`, `#[brioche_offload_task]` macros; `PluginBuilder`; `MockEngine`; prelude |
| `brioche-plugin-template` cargo-generate | ✅ | `Cargo.toml` + `src/lib.rs` template with `{{project-name}}` placeholders; excluded from workspace |
| `cargo brioche scaffold` CLI | ✅ | Implemented as `PluginBuilder::standard()` / `permissive()` / `strict()` + `bare()` modes |
| `brioche-playground` Docker image | ✅ | Mock LLM backend (`MockLlmBackend`) + effect logger (`EffectLogger`) + invariant panel; binary target |
| `brioche-docgen` trait dependency graph | ✅ | Markdown / HTML / JSON output via `brioche-docgen trait-graph --format <fmt>` |
| `brioche-docgen` sequence diagrams per `EngineInput` | ✅ | Auto-generated Mermaid sequence diagrams for all 4 `EngineInput` variants |
| `cargo-brioche-lint-invariants` | ✅ | `--check-refs` (regex validation), `--check-matrix` (placeholder), `--json` output |
| `cargo-brioche-lint` (plugin linter) | ✅ | Detects direct `session.history` / `session.state` access and `unwrap`/`expect` in plugin code |
| `brioche-plugin-test` mock engine utility | ✅ | `MockEngine` with `Permissive` profile pre-wired; no async dependency |
| Integration tests: plugin kit compile-tests | ✅ | 7 tests verifying `#[brioche_plugin]`, `#[hook]`, and `#[brioche_offload_task]` expansion and behavior |

**Key Invariants Targeted:** I-Core-ExtensionType, I-Eco-ExtensionOverMod

**Verification:**
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` ✅
- `cargo test --workspace --all-targets --all-features` ✅
- `cargo deny check all` ✅
- `cargo fmt --check` ✅

---

## Phase 7: Invariants, Verification & Release — Book V (Sprint 18)

### Sprint 18: Verification, Benchmarks, Release Engineering ⏳

| Deliverable | Status | Notes |
|-------------|--------|-------|
| `GovernanceCompatibilityMatrix` full test coverage | ⏳ | All 5 composition invariants verified |
| Property tests (`proptest`) for `transition()` | ⏳ | Never panics; bit-for-bit determinism |
| Replay tests: `AuditState` → blank engine | ⏳ | Zero divergence on 1000 replays |
| Replay tests: `TransitionJournal` → post-watchdog | ⏳ | Unpersisted transitions replayed after restart |
| Criterion benchmark suite | ⏳ | All 10 suites below threshold |
| `stream_latency` P99 | ⏳ | < 50 µs |
| `stream_zero_alloc` | ⏳ | Zero heap allocations (`Pass`/`Hold` mode) |
| `hook_effect_o1` | ⏳ | < 100 ns |
| `extension_lookup` | ⏳ | < 50 ns |
| `cross_instance_determinism` | ⏳ | Zero divergence on 10,000 replays |
| `cow_rollback` | ⏳ | < 10 µs |
| `tiered_rollback_critical` | ⏳ | < 5 µs |
| `subroutine_cache_l1` | ⏳ | < 1 µs |
| `redb_idempotence` | ⏳ | Zero divergence |
| `negotiation_broker` | ⏳ | < 50 µs |
| Release tags GPG-signed | ⏳ | `git tag -s v0.1.0` |
| All 4 compilation profiles CI-tested | ⏳ | `wasm-test`, `headless`, `desktop`, `full` |
| `cargo deny check all` clean | ⏳ | License + advisory + ban checks |
| `cargo doc --workspace --no-deps` warning-free | ⏳ | All `pub` items documented with `Refs:` |

**Key Invariants Targeted:** All 44 system invariants (see SPECS.md Book V)

**Exit Criteria:** `cargo test --workspace --all-targets --all-features` passes; all benchmarks under threshold; GPG-signed release tag; 0 `cargo doc` warnings.

---

## Sprint Reference Table

| Sprint | Phase | Book | Focus | Status |
|--------|-------|------|-------|--------|
| 0 | Foundation | — | Repo setup, CI, tooling | ✅ |
| 1 | Phase 1 | I | `brioche-macro`, spec skeleton | ✅ |
| 2 | Phase 1 | I | `ExtensionStorage` + `ExtVTable` | ✅ |
| 3 | Phase 1 | I | `Session`, `AgentState`, `SessionRegistry` | ✅ |
| 4 | Phase 1 | I | `BriocheEngine::transition()`, routing | ✅ |
| 5 | Phase 1 | I | `seal()`, `ActiveToolCall`, dispatch refinement | ✅ |
| 6 | Phase 2 | II | Fundamental governance traits | ✅ |
| 7 | Phase 2 | II | Optional traits + COW integration | ✅ |
| 8 | Phase 2 | II | `brioche-governance-default` + remaining plugins | ✅ |
| 9 | Phase 3 | III-A | Event loop, effect execution, backpressure | ✅ |
| 10 | Phase 3 | III-A | SignalMultiplexer, UnifiedEventBus, Watchdog | ✅ |
| 11 | Phase 3 | III-A | TransitionJournal, PersistenceMode, integration | ✅ |
| 12 | Phase 4 | III-B | Redb schema, SubRoutineCache, save protocol | ✅ |
| 13 | Phase 4 | III-B | Loading, rehydration, GC | ✅ |
| 14 | Phase 5 | III-C | UiRegistry, ContentRenderer, UiComposer | ✅ |
| 15 | Phase 5 | III-C | Tauri IPC, sub-routine UI, performance policy | ✅ |
| 16 | Phase 6 | IV | Standard plugins (`brioche-std`) | ✅ |
| 17 | Phase 6 | IV | Plugin kit, Playground, docgen, lint | ✅ |
| 18 | Phase 7 | V | Verification, benchmarks, release | ⏳ |

---

## Definition of Done (Applied Per Sprint)

- [ ] Code implemented and compiled with `--all-features`
- [ ] Specification updated in `/docs/architecture/` (relevant Book chapter)
- [ ] Property tests (`proptest`) added for state-space exploration (Core changes)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] `cargo deny check all` passes
- [ ] `cargo fmt` passes
- [ ] Criterion benchmark established (if touching hot path)
- [ ] GPG-signed commits on every commit in the PR
- [ ] ADR added if crossing book boundaries or modifying traits
- [ ] `brioche-docgen` output regenerated if public API changed
- [ ] Documentation updated (inline docs + architecture spec)

---

*Last updated: 2026-05-28 — Sprint 16 complete; standard plugins shipped*
