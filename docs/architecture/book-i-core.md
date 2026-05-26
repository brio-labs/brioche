# Book I — The Core Book

> Living specification of the synchronous kernel and pure mechanisms.
> Canonical source: `SPECS.md` §BOOK I. This document is updated incrementally.

---

## Chapter 1: Foundations

### 1.1 Vision and guiding principles

Brioche is a secure monolithic SDK for language model orchestration. The kernel never produces side effects — it receives a typed state and event, purely computes the next state, and returns a list of declarative intentions (*Effects*). The asynchronous shell is the only entity authorized to interact with the OS, network, persistence, and UI.

**Principles:**
- **Control inversion by effects** (Pure Core / Impure Shell)
- **Mechanism vs Policy Separation**
- **Extension over Modification**
- **Strict determinism**
- **Security by design**
- **Session !Send**
- **Governance through profiles**

### 1.2 Global topology

Five watertight architectural layers. Policy lives in extension crates registering onto mechanism hooks.

```
UI (Vue 3) ←→ Shell Projection (Tauri IPC) ←→ Shell Runtime (Tokio)
                                          ↕
Shell Persistence (Redb) ←→ Core (Synchronous Kernel) ←→ Governance (Traits)
```

### 1.3 Security model and system invariants

Key invariants enforced by Book I:

| Invariant | Code | Enforcement |
|-----------|------|-------------|
| Extension types are compile-time verified | I-Core-ExtensionType | `brioche-macro` `trybuild` tests |
| O(1) extension access by `TypeId` | I-Core-ExtO1 | `ExtensionStorage` hot_map |
| Kernel never panics | I-Core-NoPanic | `transition()` returns `Vec<Effect>` |
| Streaming hot path has no branching | I-Core-StreamNoBranch | Pre-routed `UnifiedRoutingTable` |
| Plugin evaluation order is total | I-Core-PluginOrder | `priority` + `name` deterministic sort |
| Effects are returned as `Vec<Effect>` | I-Core-RetVecEffect | Return type of `transition()` |
| VTable provides `clone_box` | I-Core-VTableClone | `BriocheExtensionType` derive |
| Streaming chunks ≤ 4KB | I-Core-ChunkBudget | `MAX_INLINE_CHUNK` |
| `ActiveToolCall.timeout_ms` materialized | I-Core-ActiveToolCall | `seal()` function |

### 1.4 Separate channels and deterministic drainage

Kernel knows only `EngineInput`. System events transit through separate channels consumed by plugins via adapters.

**Canonical drainage order:** `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`

### 1.5 Error taxonomy

Four error families:
1. **Tool business errors** (`ToolOutcome`) — data for the LLM
2. **Policy errors** (`PluginError::Soft` / `PluginError::Fatal`)
3. **System errors** (`BriocheError`)
4. **Programming panics** — caught by shell, emergency serialization

---

## Chapter 2: Fundamental types

### 2.1 Session and automaton

```rust
pub struct Session {
    pub id: String,
    pub history: Vec<ChatMessage>,
    pub persisted_msg_count: usize,
    pub state: AgentState,
    pub state_stack: Vec<AgentState>,
    pub extensions: ExtensionStorage,
    pub active_tools: Vec<ActiveToolCall>,
}
```

`Session` is strictly `!Send` and `!Sync` (enforced via `PhantomData<*mut ()>` on stable Rust). A single thread owns it.

**Methods:**
- `Session::new(id)` — creates a session in `AgentState::Idle`
- `push_state(new_state)` — pushes current state and transitions; rejects `Failure`
- `pop_state()` — pops and restores; returns `BriocheError::InvalidStateTransition` if empty
- `snapshot()` — produces a `SessionSnapshot` for plugin consumption

**Sub-routine registry (`SessionRegistry`):**
```rust
pub struct SessionRegistry {
    sessions: BTreeMap<SubRoutineHandle, Session>,
    exit_counts: BTreeMap<SubRoutineHandle, u64>,
}
```

Also `!Send + !Sync`. Methods: `insert`, `get_mut`, `remove`, `contains`, `increment_exit_count`, `get_exit_count`, `handles`.

**Lifecycle states:** `Idle` → `Predicting` → `ExecutingTools` → `SubRoutine` → `Idle` / `Failure`

### 2.2 Messages and descriptors

| Type | Role |
|------|------|
| `ChatMessage` | History entries (System, User, Assistant, ToolRequest, ToolResult) |
| `ToolCallDescriptor` | Plugin interface for tool calls (`tool_id`, `tool_name`, `arguments`, `timeout_ms: Option<u64>`) |
| `ActiveToolCall` | Kernel-internal, materialized after `seal()` (`timeout_ms: u64`, never `None`) |
| `ToolOutcome` | Business data: `Success`, `BusinessError`, `SystemError`, `TimeoutWithPartialData` |
| `ToolResultDTO` | Structured result from shell to kernel |

**`seal()` function:** Canonical conversion `Vec<ToolCallDescriptor>` → `Vec<ActiveToolCall>`. Exhaustive match enforced by compiler. Any `None` timeout defaults to `0`.

### 2.3 Engine inputs

```rust
pub enum EngineInput {
    UserMessage(String),
    LlmStream(StreamEvent),
    ToolCallsResult { generation_id: u64, results: Vec<ToolResultDTO> },
    RestoreSubRoutine { handle: SubRoutineHandle, head_blob: Vec<u8> },
}
```

System signals, async results, and governance notifications transit through separate channels (see §1.4). They are **never** variants of `EngineInput`.

### 2.4 Declarative effects

**Policy decisions (plugin → core):**
```rust
pub enum PolicyDecision {
    Allow,
    Block { reason: String },
    MutateHistory(Vec<HistoryEdit>),
    RequestEffect(Effect),
    OverrideTransition(Vec<Effect>),
}

pub enum HistoryEdit {
    Insert { index: usize, message: ChatMessage },
    Replace { index: usize, message: ChatMessage },
    Truncate { keep_last: usize },
}
```

**Effects (core → shell):**
```rust
pub enum Effect {
    CallLlmNetwork,
    ExecuteTools(Vec<ActiveToolCall>),
    ForwardToUi { widget_type: String, payload: serde_json::Value },
    Error { code: ErrorCode, message: String },
    SaveSession,
    SavePluginBlob { plugin_id: String, data: Vec<u8> },
    TriggerSummarization,
    ExecuteCpuTask { task_id: String, payload: Vec<u8> },
    TriggerGc,
    SystemIdle,
    PluginFault { plugin_name: String, error: PluginError },
    RebuildRoutes,
    SubRoutineRestored { handle: SubRoutineHandle },
}

pub enum ErrorCode {
    NetworkUnavailable,
    OperationCancelled,
    StateInconsistency,
    EpochMismatch,
    PluginFaulted,
}
```

`Effect` contains **only** pure mechanical effects. No telemetry, UI fallback, or specific notification variants.

### 2.5 Streaming

`bytes::Bytes` for zero-copy fragments. `MAX_INLINE_CHUNK = 4096` enforced by SSE segmentation in shell.

```rust
pub struct ExecutionPath {
    pub nodes: Vec<String>,
}

pub enum StreamEvent {
    TextChunk { path: ExecutionPath, chunk: bytes::Bytes },
    ToolCallStart { path: ExecutionPath, id: String, name: String },
    ToolArgumentChunk { path: ExecutionPath, id: String, chunk: bytes::Bytes },
    ToolCallDone { path: ExecutionPath },
    Pass,
}

pub enum StreamAction {
    Pass,
    Hold,
    OffloadTask { task_id: String, payload: Vec<u8> },
}
```

---

## Chapter 3: ExtensionStorage and extension types

### 3.1 Architecture

`ExtensionStorage` guarantees O(1) access by `TypeId` with binary persistence.

**Internal architecture:**
- `hot_map`: `HashMap<TypeId, Box<dyn Any + Send + Sync>>` — typed runtime access
- `cold_snapshot`: `BTreeMap<String, Vec<u8>>` — binary persistence by `EXT_ID`
- `registry`: `HashMap<TypeId, ExtVTable>` — (de)serialization, cloning, default construction

**Procedures:**
- `insert<T>`: serialize to blob, store in cold_snapshot, place typed instance in hot_map
- `get_mut<T>`: downcast from hot_map; if `CycleRollbackPolicy` is active, triggers COW clone at first write
- `get_or_insert_default<T>`: infallible — restores from cold_snapshot or injects default
- `hydrate_plugin`: restores specific plugin from raw blob; failure resets only that plugin

**Extended VTable:**
```rust
pub struct ExtVTable {
    pub ext_id: &'static str,
    pub serialize: fn(&dyn Any) -> Vec<u8>,
    pub deserialize: fn(&[u8]) -> Result<Box<dyn Any + Send + Sync>, String>,
    pub clone_box: fn(&dyn Any) -> Box<dyn Any + Send + Sync>,
    pub estimated_weight_bytes: fn(&dyn Any) -> usize,
    pub snapshot_strategy: SnapshotStrategy,
    pub default_construct: fn() -> Box<dyn Any + Send + Sync>,
}
```

### 3.2 Compile-time verification proc-macro

`#[derive(BriocheExtensionType)]` mechanically guarantees extension compliance.

**Static verifications:**
1. **Presence of `EXT_ID`** — auto-generated from `module_path!()` + type name; respects `crate::type_name` format
2. **Prohibition of `HashMap`/`HashSet`** — recursive field analysis; compilation fails if persisted fields contain these types
3. **Absence of UI types** — detects `tauri`, `vue`, `dom` crate imports in struct fields
4. **Determinism of `Vec`s** — on stable Rust the macro emits `compile_error!` for any `Vec<T>` field not annotated with `#[brioche(deterministic_order)]`. (When `compile_warning!` stabilizes, this will become a warning that is deny-by-default under the `strict-determinism` feature.)
5. **`clone_box` generation** — requires `Clone`; compilation fails if type cannot derive/impl `Clone`
6. **`estimated_weight_bytes` generation** — estimates weight via binary serialization
7. **`snapshot_strategy` generation** — `FullClone` by default; `#[brioche(no_snapshot)]`, `#[brioche(incremental_snapshot)]`, `#[brioche(critical_state)]` annotations modify this
8. **Sealed trait** — `BriocheExtensionType` is sealed; only the proc-macro can emit implementations

### 3.3 Standard extension types

Governance-critical types carry `#[brioche(critical_state)]`:
- `EpochState`
- `TransitionTraceLog`
- `SupersededTransitionTraceLog`
- `CycleBudgetState`
- `SubRoutineTimerState`
- `HookEffectConstraintState`

Business types (e.g., `TokenTrackerState`) do not carry `critical_state` by default.

---

## Chapter 4: Plugin interface

### 4.1 `BriochePlugin` trait and `PluginCapabilities`

Plugins declare their hook subscriptions via a bitmask. At engine initialization, the `UnifiedRoutingTable` pre-computes routes for each capability, eliminating runtime mask checks in the hot path.

```rust
pub struct PluginCapabilities(pub u16);

impl PluginCapabilities {
    pub const NONE: Self = Self(0);
    pub const ON_INPUT: Self = Self(1 << 0);
    pub const BEFORE_PREDICTION: Self = Self(1 << 1);
    pub const ON_STREAM_EVENT: Self = Self(1 << 2);
    pub const AFTER_PREDICTION: Self = Self(1 << 3);
    pub const ON_TOOL_CALLS: Self = Self(1 << 4);
    pub const ON_TOOL_RESULT: Self = Self(1 << 5);
    pub const ON_ERROR: Self = Self(1 << 6);
}
```

```rust
pub trait BriochePlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> PluginCapabilities;
    fn priority(&self) -> i16 { 0 }
    fn on_input(&self, input: &EngineInput, ext: &mut ExtensionStorage)
        -> PluginResult<PolicyDecision>;
    fn before_prediction(&self, history: &[ChatMessage], ext: &mut ExtensionStorage)
        -> PluginResult<PolicyDecision>;
    fn on_stream_event(&self, event: &StreamEvent, ext: &mut ExtensionStorage)
        -> PluginResult<StreamAction>;
    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()>;
    fn on_tool_calls(&self, calls: &mut Vec<ToolCallDescriptor>, ext: &mut ExtensionStorage)
        -> PluginResult<()>;
    fn on_tool_result(&self, results: &mut Vec<ToolResultDTO>, ext: &mut ExtensionStorage)
        -> PluginResult<()>;
    fn on_error(&self, error: &PluginError, ext: &mut ExtensionStorage)
        -> PluginResult<PolicyDecision>;
}
```

All hooks have default implementations returning "allow/pass/ok", so a plugin only overrides the hooks it cares about.

### 4.2 Policy decisions

`PolicyDecision` is the plugin → core contract:
- `Allow` — proceed
- `Block { reason }` — stop, return error + idle
- `MutateHistory(Vec<HistoryEdit>)` — modify session history
- `RequestEffect(Effect)` — ask kernel to emit an effect (validated by `HookEffectConstraint`)
- `OverrideTransition(Vec<Effect>)` — force state transition and emit effects immediately

### 4.3 Plugin error handling

- `Soft` — logged, next plugin evaluated, session continues
- `Fatal` — kernel emits `Effect::PluginFault`; governance plugin (e.g., `QuarantineManager`) decides follow-up

---

## Chapter 5: Transition algorithm

### 5.1 `BriocheEngine::transition(session, input) -> Vec<Effect>`

The kernel exposes `SessionSnapshot` in `ExtensionStorage` before each transition cycle. The algorithm (pure mechanism):

**1. Inject `SessionSnapshot`** before each hook.

**2. `EpochInterceptor`** (optional, evaluated first):
- `Block { reason }` → return `Error(EpochMismatch)` + `SystemIdle`
- `Proceed` → continue

**3. `SubRoutineHandler`** (optional):
- If `session.state` is `SubRoutine(handle)`, resolve child via `SessionRegistry`
- If `Some(effects)` → return immediately (short-circuit standard dispatch)
- If `None` → continue

**4. `on_input` hook** (pre-routed):
- Evaluate plugins in `(priority, name)` order
- `OverrideTransition` — first wins, logged in `TransitionTraceLog`, subsequent ones logged as superseded
- `Block` → return `Error(StateInconsistency)` + `SystemIdle`
- `MutateHistory` / `RequestEffect` — accumulate

**5. Main dispatch on `EngineInput`:**

| Input | Action |
|-------|--------|
| `UserMessage` | Push to history → `push_state(Predicting)` → `before_prediction` hook → `DecisionAggregator` → `CallLlmNetwork` + `SaveSession` |
| `LlmStream` | If not `Predicting`, return `[]`. Route `on_stream_event` to plugins. `Pass`/`Hold`/`OffloadTask` → effects. |
| `ToolCallsResult` | `pop_state()` → clear `active_tools` → `on_tool_result` hook → push results to history → `push_state(Predicting)` → `CallLlmNetwork` + `SaveSession` |
| `RestoreSubRoutine` | Register child in `SessionRegistry` → `SubRoutineRestored` + `SaveSession` |

**6. `SubRoutineLifecycleGuard`** (mandatory):
- If previous state was `SubRoutine` and current is not, call `on_exit(handle, parent, registry)`

**7. `HookEffectConstraint`** (optional):
- For each `RequestEffect`, validate via `is_allowed_fast(hook_index, effect_mask)` — O(1)
- If disallowed, replace with `Error(StateInconsistency)`

**8. `RebuildRoutes` position guarantee:**
- `RebuildRoutes` must occupy the last position in the effects vector; anything after it is truncated

**9. `ConsistencyVerifier`** (optional):
- If `Some(effects)` and no `RebuildRoutes` present → append verifier effects

**10. `GovernanceFailoverHandler`** (optional):
- If `PluginFault` on governance plugin and no `RebuildRoutes` → call handler

### 5.2 `UnifiedRoutingTable`

Pre-computed at engine initialization:

```rust
pub struct UnifiedRoutingTable {
    pub route_on_input: Vec<usize>,
    pub route_before_prediction: Vec<usize>,
    pub route_on_stream_event: Vec<usize>,
    pub route_after_prediction: Vec<usize>,
    pub route_on_tool_calls: Vec<usize>,
    pub route_on_tool_result: Vec<usize>,
    pub route_on_error: Vec<usize>,
}
```

Plugins are sorted by ascending `priority`, then by `name` lexicographically for total deterministic order. Routes contain indices into the plugin vector. The streaming loop iterates directly — no branching on bitmasks.

**Complexity:** O(p log p) at init (once), O(1) per plugin in the hot path.

### 5.3 Governance traits (anchor points)

The kernel defines 11 governance trait slots:

| # | Trait | Mandatory | Role |
|---|-------|-----------|------|
| 1 | `EpochInterceptor` | No | Temporal barrier — rejects stale epochs |
| 2 | `SubRoutineHandler` | No | Delegates sub-routine input resolution |
| 3 | `ConsistencyVerifier` | No | Post-transition mechanical validation |
| 4 | `DecisionAggregator` | **Yes** | Merges `before_prediction` decisions |
| 5 | `SignalDrainOrder` | No* | Defines invariant channel drainage order |
| 6 | `CycleBudgetPolicy` | No | Per-plugin synchronous cycle budget |
| 7 | `HookEffectConstraint` | No | O(1) effect permission validation |
| 8 | `CycleRollbackPolicy` | No | Granular COW rollback on budget overrun |
| 9 | `SubRoutineLifecycleGuard` | **Yes** | Cleanup on outgoing `SubRoutine` transition |
| 10 | `GovernanceFailoverHandler` | No | Safety net for cascading governance failures |
| 11 | `CowBudgetPolicy` | No | Per-hook COW budget for rollback |

\* Mandatory for the shell; the kernel delegates to it but does not start without a shell.

### 5.4 `BriocheEngineBuilder`

```rust
pub struct BriocheEngineBuilder { ... }

impl BriocheEngineBuilder {
    pub fn new() -> Self;
    pub fn with_plugin(self, plugin: Box<dyn BriochePlugin>) -> Self;
    pub fn with_epoch_interceptor(self, interceptor: Box<dyn EpochInterceptor>) -> Self;
    pub fn with_subroutine_handler(self, handler: Box<dyn SubRoutineHandler>) -> Self;
    pub fn with_consistency_verifier(self, verifier: Box<dyn ConsistencyVerifier>) -> Self;
    pub fn with_decision_aggregator(self, aggregator: Box<dyn DecisionAggregator>) -> Self;
    pub fn with_cycle_budget_policy(self, policy: Box<dyn CycleBudgetPolicy>) -> Self;
    pub fn with_hook_effect_constraint(self, constraint: Box<dyn HookEffectConstraint>) -> Self;
    pub fn with_cycle_rollback_policy(self, policy: Box<dyn CycleRollbackPolicy>) -> Self;
    pub fn with_subroutine_lifecycle_guard(self, guard: Box<dyn SubRoutineLifecycleGuard>) -> Self;
    pub fn with_governance_failover_handler(self, handler: Box<dyn GovernanceFailoverHandler>) -> Self;
    pub fn with_cow_budget_policy(self, policy: Box<dyn CowBudgetPolicy>) -> Self;
    pub fn build(self) -> Result<BriocheEngine, BriocheError>;
}
```

`build()` enforces mandatory traits:
- `DecisionAggregator` is required
- `SubRoutineLifecycleGuard` is required

---

## Chapter 6: Limits of the Core layer

*To be written in Sprint 5. See `SPECS.md` §6 for canonical content.*

---

## Invariant Traceability

| Invariant | Sprint | Status |
|-----------|--------|--------|
| I-Core-ExtensionType | 1 | ✅ Complete |
| I-Core-ExtO1 | 2 | ✅ Complete |
| I-Core-Pure | 3 | ✅ Complete |
| I-Core-NoPanic | 3 | ✅ Complete |
| I-Core-StreamNoBranch | 4 | ✅ Complete |
| I-Core-PluginOrder | 4 | ✅ Complete |
| I-Core-RetVecEffect | 4 | ✅ Complete |
| I-Core-ChunkBudget | 5 | ⬜ Pending |
| I-Core-ActiveToolCall | 5 | ⬜ Pending |
| I-Core-VTableClone | 2 | ✅ Complete |
| I-Core-ChunkBudget | 5 | ⬜ Pending |
| I-Core-ActiveToolCall | 5 | ⬜ Pending |

---
