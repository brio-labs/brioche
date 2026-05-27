# ADR-001: `rebuild_routes()` Transactional Barrier and `TransitionJournal` Shell-Side Placement

## Status
Accepted

## Context

Sprint 11 introduces two features that cross the Core / Shell Runtime boundary:

1. **`BriocheEngine::rebuild_routes(active_mask)`**: a mechanism to recalculate the `UnifiedRoutingTable` without restarting the engine. This is triggered by `Effect::RebuildRoutes` emitted from governance plugins (e.g. `QuarantineManager`) and must be executed as a transactional barrier — no new `EngineInput` may enter the kernel until recalculation completes.

2. **`TransitionJournal`**: a lock-free ring buffer that persists every `EngineInput` before `transition()` is called. This enables the `EngineWatchdog` to replay unpersisted transitions after an engine restart.

Both features required decisions about where state lives, which layer owns the concern, and how the boundary contract is maintained.

## Decision

### 1. `rebuild_routes` lives in Core as pure mechanism

The `rebuild_routes(&mut self, active_mask: &[bool])` method is implemented on `BriocheEngine` (Core, Book I). It receives a boolean mask from the shell and recalculates routing vectors in O(p log p) using the existing `(priority, name)` sort key. The kernel knows nothing about *why* a plugin is excluded (quarantine, user action, etc.) — it only sees a mechanical mask.

**Rationale**: Route recalculation is a mechanical operation (index filtering + resorting). Policy decisions about *which* plugins to exclude belong in governance plugins. The shell merely transports the `RebuildRoutes` effect to the engine thread and waits for completion.

### 2. Transactional barrier is implemented in Shell Runtime, not Core

The barrier is enforced by an `AtomicBool` (`rebuild_in_progress`) in `BriocheShell`. When `Effect::RebuildRoutes` is received:

1. The async effect loop sets `rebuild_in_progress = true`.
2. It sends a `RebuildCommand` (containing the active mask + a `oneshot::Sender`) to the engine thread.
3. `BriocheShell::send_input()` returns `Err(ShellError::RebuildInProgress)` while the flag is set.
4. The engine thread processes the command, calls `engine.rebuild_routes()`, and signals completion via the oneshot channel.
5. The effect loop clears `rebuild_in_progress = false`.

**Rationale**: The Core is synchronous and single-threaded; it has no concept of "blocking" async callers. The Shell Runtime owns the channel infrastructure and is the natural place to implement the barrier. This preserves the Core's purity (no async, no locks).

### 3. `TransitionJournal` is a Shell Runtime component, not Core

The `TransitionJournal` is created in `BriocheShell::new()` and passed as an `Arc` to both the engine thread (writer) and the watchdog task (reader). The engine thread calls `journal.append(&input)` before each `transition()`. The watchdog may call `read_unacknowledged()` after detecting a non-responsive engine.

**Rationale**: Persistence of inputs for recovery is an operational concern, not a kernel concern. The Core remains agnostic of the journal's existence. This respects the Mechanism/Policy separation: the kernel computes transitions; the shell manages durability and recovery.

### 4. `NetworkRecovery` is a Shell Runtime trait, never visible to Core

Network failures are handled entirely within `DefaultEffectExecutor::call_llm()`. The retry loop consults a `NetworkRecovery` trait implementation (`ExponentialBackoff` or `NoRetry`). Only after all retries are exhausted does the shell emit `SystemSignal::NetworkUnavailable` into the kernel's separate channel.

**Rationale**: Transport-level retry/backoff is I/O policy. The kernel must not see partial failures, connection timeouts, or HTTP status codes. This upholds I-Shell-Network-Signal.

## Consequences

### Positive

- Core remains pure, synchronous, and agnostic of recovery and barrier semantics.
- Shell Runtime can evolve retry policies, journal formats, and barrier strategies without Core changes.
- `rebuild_routes` can be unit-tested in Core without async infrastructure.
- `TransitionJournal`'s single-writer/single-reader design requires no locks, matching the engine thread's `!Send` invariant.

### Negative

- The barrier introduces a brief window (~microseconds) where `send_input()` returns an error. Callers (IPC handlers) must handle `ShellError::RebuildInProgress` gracefully.
- `TransitionJournal` uses `UnsafeCell` and `unsafe impl Send/Sync`. The safety argument relies on the single-writer/single-reader protocol enforced by atomic indices. Any future change to multiple writers or readers would break safety.

### Neutral

- `postcard` is added as a dependency of `brioche-shell-runtime` for binary serialization. It was already a dependency of `brioche-core`, so the workspace lockfile change is minimal.

## Affected Invariants

| Invariant | Impact |
|-----------|--------|
| I-Shell-TransitionJournal | **New** — every `EngineInput` persisted before `transition()` |
| I-Gov-Rebuild-Barrier | **New** — route recalculation is a transactional barrier |
| I-Shell-Persistence-Mode | **New** — `SaveSession` flush behavior controlled by shell |
| I-Shell-Network-Signal | **Upheld** — transport errors never reach kernel directly |
| I-Core-Pure | **Upheld** — Core performs no I/O, no async, no barrier logic |
| I-Shell-Session-NoSend | **Upheld** — `Session` never crosses thread boundaries |

## Book References

- SPECS.md §Book I Ch 6.2 — Interface contract with Governance layer (mandatory traits)
- SPECS.md §Book I Ch 6.3 — Interface contract with Shell (SignalDrainOrder, TransitionJournal)
- SPECS.md §Book III-A Ch 1.1 — Shell startup procedure (9 steps)
- SPECS.md §Book III-A Ch 1.3 — Effect execution (`RebuildRoutes` as transactional barrier)
- SPECS.md §Book III-A Ch 4 — EngineWatchdog bi-directional heartbeat
- PHILOSOPHY.md §2.1 — Mechanism vs Policy in Code
- PHILOSOPHY.md §4.4 — Architecture Decision Records in Code
