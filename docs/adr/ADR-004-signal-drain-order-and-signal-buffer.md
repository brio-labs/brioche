# ADR-004: `SignalDrainOrder` and `SignalBuffer`

## Status

Accepted

## Context

The kernel knows only `EngineInput`. System events (`SystemSignal`), governance notifications (`GovernanceNotification`), and asynchronous task results (`AsyncTaskResult`) transit through separate channels that must never become variants of `EngineInput`. The shell is responsible for draining those channels between transition cycles and making pending events available to governance plugins.

This creates a boundary problem:

- `brioche-core` must define the contract so the engine thread can call `drain()` before every `transition()`, but it cannot depend on `brioche-shell-runtime`.
- `brioche-shell-runtime` owns the channel receivers and knows the canonical order, but it cannot depend on internal plugin APIs.
- Plugins need a typed, deterministic view of pending signals inside `ExtensionStorage`.
- The buffer is reconstructed every cycle, so snapshotting or rolling it back is meaningless.

## Decision

### 1. Define the drainage contract in `brioche-core`

Two types live in `brioche-core::types::runtime`:

```rust
pub trait SignalDrainOrder: Send + Sync {
    fn drain(&self) -> SignalDrainBatch;
}

pub struct SignalDrainBatch {
    pub system_signals: Vec<SystemSignal>,
    pub governance_notifications: Vec<GovernanceNotification>,
    pub async_task_results: Vec<AsyncTaskResult>,
}

#[brioche(no_snapshot)]
pub struct SignalBuffer {
    pub system_signals: Vec<SystemSignal>,
    pub governance_notifications: Vec<GovernanceNotification>,
    pub async_task_results: Vec<AsyncTaskResult>,
}
```

**Rationale**: The kernel only needs a batch in a canonical order. Keeping the trait and buffer in Core lets the engine loop call `drain()` and inject `SignalBuffer` without knowing about Tokio receivers or shell internals.

### 2. Implement `SignalDrainOrder` in `brioche-shell-runtime`

`SignalMultiplexer` owns the three channel receivers and drains them atomically with respect to transition cycles:

- `SystemSignal` is fully drained first.
- `GovernanceNotification` is fully drained second.
- `AsyncTaskResult` is fully drained third.
- FIFO order is preserved within each channel.

An alternative `UnifiedEventBus` is also provided for shells that prefer a single internal envelope channel; it still produces a `SignalDrainBatch` in the same canonical order.

**Rationale**: The shell owns the channels and the thread boundary. Placing the implementation in the runtime crate preserves the dependency direction (runtime depends on core, not the reverse) and lets the runtime choose between adapter-based or unified event delivery.

### 3. Inject `SignalBuffer` into `ExtensionStorage` before each transition

The shell engine loop calls `signal_drain.drain()`, then inserts the resulting `SignalBuffer` into `session.extensions` before invoking `BriocheEngine::transition()`. Plugins read from the buffer in their hooks; the buffer is cleared and repopulated each cycle.

**Rationale**: `ExtensionStorage` is the already-isolated plugin state mechanism. Reusing it avoids a new side-channel API and keeps plugin access deterministic and typed.

### 4. Mark `SignalBuffer` as `#[brioche(no_snapshot)]`

Because the buffer is fully reconstructed from external channels every cycle, rolling it back or persisting it is meaningless. The `no_snapshot` annotation excludes it from COW snapshots and persistence.

**Rationale**: Snapshotting transient shell-derived state would violate the principle that only kernel-derived state is rolled back. The annotation makes this explicit and avoids wasted memory.

## Consequences

### Positive

- Core remains independent of async channel implementation.
- The dependency graph is preserved: `brioche-core` ← `brioche-shell-runtime`.
- Drainage order is materialized in code, not configuration.
- Plugins consume pending signals through the same typed `ExtensionStorage` API used for all state.

### Negative

- `brioche-core` gains two more types (`SignalDrainBatch`, `SignalBuffer`) tied to the shell event model.
- The shell must ensure the buffer is inserted before every transition; forgetting it would make plugins silently miss signals.

### Neutral

- The canonical order (`SystemSignal`, `GovernanceNotification`, `AsyncTaskResult`) is a design choice that must be respected by any future `SignalDrainOrder` implementation.

## Affected Invariants

| Invariant | Impact |
|-----------|--------|
| I-Shell-Drain-Atomic | **Upheld** — drainage is atomic with respect to a transition cycle. |
| I-Core-Pure | **Upheld** — Core defines the contract but performs no I/O or channel access. |
| I-Shell-Session-NoSend | **Upheld** — `SignalBuffer` is constructed on the engine thread and stored in `ExtensionStorage`. |
| I-Shell-Runtime-DeterministicClock | **Upheld** — `SystemSignal::Tick` ordering is preserved by FIFO drainage. |

## Book References

- docs/SPECS.md §1.4 — Separate channels and deterministic drainage
- docs/SPECS.md §Book III-A Ch 1 — Event loop and orchestration
- docs/SPECS.md §Book III-A Ch 3 — `SignalMultiplexer`
- docs/architecture/book-ii-governance.md §2.5 — `SignalDrainOrder`
- PHILOSOPHY.md §2.1 — Mechanism vs Policy in Code
