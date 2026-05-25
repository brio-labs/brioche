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

impl !Send for Session {}
impl !Sync for Session {}
```

**Sub-routine registry (`SessionRegistry`):**
```rust
pub struct SessionRegistry {
    sessions: BTreeMap<SubRoutineHandle, Session>,
    exit_counts: BTreeMap<SubRoutineHandle, u64>,
}
impl !Send for SessionRegistry {}
impl !Sync for SessionRegistry {}
```

**Lifecycle states:** `Idle` → `Predicting` → `ExecutingTools` → `SubRoutine` → `Idle` / `Failure`

### 2.2 Messages and descriptors

| Type | Role |
|------|------|
| `ChatMessage` | History entries (System, User, Assistant, ToolRequest, ToolResult) |
| `ToolCallDescriptor` | Plugin interface for tool calls |
| `ActiveToolCall` | Kernel-internal, materialized after `seal()` |
| `ToolOutcome` | Business data: Success, BusinessError, SystemError, TimeoutWithPartialData |
| `ToolResultDTO` | Structured result from shell to kernel |

**`seal()` function:** Canonical conversion `Vec<ToolCallDescriptor>` → `Vec<ActiveToolCall>`. Exhaustive match enforced by compiler.

### 2.3 Engine inputs

```rust
pub enum EngineInput {
    UserMessage(String),
    LlmStream(StreamEvent),
    ToolCallsResult { generation_id: u64, results: Vec<ToolResultDTO> },
    RestoreSubRoutine { handle: SubRoutineHandle, head_blob: Vec<u8> },
}
```

System signals, async results, and governance notifications transit through separate channels (see §1.4).

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
```

### 2.5 Streaming

`bytes::Bytes` for zero-copy fragments. `MAX_INLINE_CHUNK = 4096` enforced by SSE segmentation in shell.

```rust
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
    pub serialize: fn(&dyn Any) -> Vec<u8>,
    pub deserialize: fn(&[u8]) -> Box<dyn Any>,
    pub clone_box: fn(&dyn Any) -> Box<dyn Any>,
    pub estimated_weight_bytes: fn(&dyn Any) -> usize,
    pub snapshot_strategy: SnapshotStrategy,
    pub default_construct: fn() -> Box<dyn Any>,
}
```

### 3.2 Compile-time verification proc-macro

`#[derive(BriocheExtensionType)]` mechanically guarantees extension compliance.

**Static verifications:**
1. **Presence of `EXT_ID`** — auto-generated from `module_path!()` + type name; respects `crate::type_name` format
2. **Prohibition of `HashMap`/`HashSet`** — recursive field analysis; compilation fails if persisted fields contain these types
3. **Absence of UI types** — detects `tauri`, `vue`, `dom` crate imports in struct fields
4. **Determinism of `Vec`s** — warns (deny-by-default under `strict-determinism`) on undetermined `Vec` fields
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

*To be written in Sprint 2. See `SPECS.md` §4 for canonical content.*

## Chapter 5: Transition algorithm

*To be written in Sprint 4. See `SPECS.md` §5 for canonical content.*

## Chapter 6: Limits of the Core layer

*To be written in Sprint 5. See `SPECS.md` §6 for canonical content.*

---

## Invariant Traceability

| Invariant | Sprint | Status |
|-----------|--------|--------|
| I-Core-ExtensionType | 1 | 🔄 In Progress |
| I-Core-ExtO1 | 2 | ⬜ Pending |
| I-Core-Pure | 3 | ⬜ Pending |
| I-Core-NoPanic | 3 | ⬜ Pending |
| I-Core-StreamNoBranch | 4 | ⬜ Pending |
| I-Core-PluginOrder | 4 | ⬜ Pending |
| I-Core-RetVecEffect | 5 | ⬜ Pending |
| I-Core-VTableClone | 1 | 🔄 In Progress |
| I-Core-ChunkBudget | 5 | ⬜ Pending |
| I-Core-ActiveToolCall | 5 | ⬜ Pending |

---
