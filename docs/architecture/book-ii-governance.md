# Book II — The Governance Book

> Governance layer specification (Sprints 6–8).
> This document is incremental: each sprint completes the corresponding chapters.

---

## Chapter 1: Governance Principles

The Governance layer implements **policy** (business rules) without modifying the **mechanism** (Core).

Fundamental principles:

* **Atomic traits**: each capability is a standalone trait (`EpochInterceptor`, `SubRoutineHandler`, etc.). No inheritance, no `BasePlugin`.
* **Builder injection**: `BriocheEngineBuilder` forces injection of mandatory traits at `build()` time. A missing trait = compilation error or `Err` at runtime.
* **No direct mutation of `Session`**: plugins never directly modify `session.state`, `session.history` or `session.active_tools`. They return `PolicyDecision`s or mutate their own state in `ExtensionStorage`.
* **Total trait order**: the call order in `transition()` is invariant and materialized by code, not by dynamic configuration.

---

## Chapter 2: Fundamental Traits

### 2.1 EpochInterceptor

```rust
pub trait EpochInterceptor: Send + Sync {
    fn intercept_epoch(&self, input: &EngineInput, ext: &mut ExtensionStorage)
        -> PluginResult<EpochAction>;
}
```

Evaluated **first** in each cycle. If `Block`, the kernel immediately returns `Effect::Error(EpochMismatch)` + `SystemIdle`. No subsequent trait can override an epoch barrier.

**Reference implementation**: `EpochGuard` (`brioche-governance-default`).

### 2.2 SubRoutineHandler

```rust
pub trait SubRoutineHandler: Send + Sync {
    fn handle_subroutine(&self, parent: &mut Session, child: &mut Session, input: &EngineInput)
        -> PluginResult<Option<Vec<Effect>>>;
}
```

Evaluated if `session.state == SubRoutine`. If `Some(effects)`, standard dispatch is short-circuited. Evaluated **after** `EpochInterceptor` (I-Comp-Epoch-Subroutine).

**Reference implementation** : `SubRoutineOrchestrator` (`brioche-governance-default`).

### 2.3 ConsistencyVerifier

```rust
pub trait ConsistencyVerifier: Send + Sync {
    fn verify_consistency(&self, session: &Session)
        -> PluginResult<Option<PolicyDecision>>;
}
```

Evaluated **last** in `finalize_transition`. Implementations must not mutate `session`. If the verifier returns `Some(PolicyDecision::OverrideTransition(effects))`, the kernel applies mechanical forcing (transition to `Idle`, clear `state_stack`, clear `active_tools`) and appends the returned effects. Other `PolicyDecision` variants are handled by the kernel's decision machinery. Ignored if `RebuildRoutes` is present in the effects.

**Reference implementation** : `StateConsistencyGuard` (`brioche-governance-default`).

### 2.4 DecisionAggregator

```rust
pub trait DecisionAggregator: Send + Sync {
    fn aggregate_decisions(&self, decisions: Vec<PolicyDecision>, ext: &mut ExtensionStorage)
        -> PluginResult<PolicyDecision>;
}
```

**Mandatory.** Aggregates decisions collected on `before_prediction`. Without an injected implementation, `BriocheEngineBuilder::build()` returns `Err`.

**Reference implementation** : `LexicographicDecisionAggregator` (`brioche-governance-default`).

Merge rule:
1. `Block` → short-circuits immediately.
2. `OverrideTransition` → first encountered wins.
3. `MutateHistory` → accumulation in evaluation order.
4. `RequestEffect` → first returned.
5. `Allow` → ignored.

### 2.5 SignalDrainOrder

```rust
pub trait SignalDrainOrder: Send + Sync {
    fn drain(&self) -> SignalDrainBatch;
}
```

**Mandatory.** Defines the invariant drain order of separate channels: `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`. Implemented by the shell (`SignalMultiplexer` or `UnifiedEventBus`), not by the kernel. The returned `SignalDrainBatch` is injected into `ExtensionStorage` as `SignalBuffer` before each transition cycle.

### 2.6 HookEffectConstraint

```rust
pub trait HookEffectConstraint: Send + Sync {
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;
    fn is_allowed_fallback(&self, hook_name: &str, effect_variant: &str) -> bool;
}
```

**Optional.** O(1) validation by binary mask. Without injection, all `RequestEffect`s are allowed on all hooks.

**Reference implementation** : `FastHookEffectConstraint` (`brioche-governance-default`).

### 2.7 CycleRollbackPolicy

```rust
pub trait CycleRollbackPolicy: Send + Sync {
    fn begin_hook(&mut self, hook_name: &'static str);
    fn on_mutation(&mut self, type_id: TypeId, vtable: &ExtVTable, current: &dyn Any);
    fn commit_hook(&mut self, ext: &mut ExtensionStorage);
    fn rollback_hook(&mut self, ext: &mut ExtensionStorage);
    fn is_budget_exceeded(&self) -> bool { false }
    fn set_cow_budget_policy(&mut self, _policy: Box<dyn CowBudgetPolicy>) {}
}
```

**Optional.** Provides a granular COW (Copy-On-Write) mechanism to restore the state of `ExtensionStorage` in case of budget overrun. Without injection, the kernel emits `PluginFault` without restoration. The kernel calls `begin_hook` before each monitored hook, `on_mutation` on the first write to an extension, and either `commit_hook` or `rollback_hook` at the end of the hook depending on `is_budget_exceeded`.

**Reference implementation** : `NoopCycleRollbackPolicy` (`brioche-governance-default`).

### 2.8 SubRoutineLifecycleGuard

```rust
pub trait SubRoutineLifecycleGuard: Send + Sync {
    fn on_exit(&self, handle: SubRoutineHandle, parent: &mut Session, registry: &mut SessionRegistry)
        -> PluginResult<Vec<Effect>>;
}
```

**Mandatory.** Called by the kernel on every outgoing transition from `SubRoutine`. The reference implementation `SubRoutineCleanupGuard` removes the child from `SessionRegistry` and tracks per-handle exit counters in its governance-owned `SubRoutineExitState` extension state, emitting `SaveSession` on successful removal.

**Reference implementation** : `SubRoutineCleanupGuard` (`brioche-governance-default`).

### 2.9 GovernanceFailoverHandler

```rust
pub trait GovernanceFailoverHandler: Send + Sync {
    fn handle_failure(&self, session: &mut Session, fault: &Effect)
        -> PluginResult<Option<Vec<Effect>>>;
}
```

**Optional.** Safety net in case of cascading failure of a governance plugin.

**Reference implementation** : `SystemFailoverGuard` (`brioche-governance-default`).

### 2.10 CowBudgetPolicy

```rust
pub trait CowBudgetPolicy: Send + Sync {
    fn max_cow_bytes(&self, hook_name: &str) -> usize;
}
```

**Optional.** Memory budget per hook for COW snapshot. Without injection, the default value is 64 KB.

---

## Chapter 3: Integration into the Engine

### 3.1 Evaluation order in `transition()`

The order is invariant and hard-coded in `BriocheEngine::transition()`:

1. **Inject `SessionSnapshot`** into `ExtensionStorage`.
2. **`EpochInterceptor`** — if `Block`, immediate return.
3. **`SubRoutineHandler`** — if in `SubRoutine` and `Some(effects)`, short-circuit.
4. **`on_input` hook** — pre-computed route.
5. **Main dispatch** (`UserMessage`, `LlmStream`, `ToolCallsResult`, `RestoreSubRoutine`).
6. **`SubRoutineLifecycleGuard`** — on exit from `SubRoutine`.
7. **`HookEffectConstraint`** — validation of emitted effects.
8. **`RebuildRoutes` position guarantee** — truncates everything after it.
9. **`ConsistencyVerifier`** — except if `RebuildRoutes` present.
10. **`GovernanceFailoverHandler`** — replaces `PluginFault` if injected.

### 3.2 Builder mandatory traits

`BriocheEngineBuilder::build()` returns `Err` if:
- `DecisionAggregator` missing
- `SubRoutineLifecycleGuard` missing

All other traits are optional.

---

## Chapter 4: Default Implementations

The `brioche-governance-default` crate provides the reference implementations:

| Trait | Implementation | File |
|-------|----------------|------|
| `EpochInterceptor` | `EpochGuard` | `guards.rs` |
| `DecisionAggregator` | `LexicographicDecisionAggregator` | `aggregators.rs` |
| `SubRoutineLifecycleGuard` | `SubRoutineCleanupGuard` | `subroutines.rs` |
| `ConsistencyVerifier` | `StateConsistencyGuard` | `guards.rs` |
| `HookEffectConstraint` | `FastHookEffectConstraint` | `aggregators.rs` |
| `CycleRollbackPolicy` | `NoopCycleRollbackPolicy` | `noop_traits.rs` |
| `GovernanceFailoverHandler` | `SystemFailoverGuard` | `guards.rs` |
| `SubRoutineHandler` | `SubRoutineOrchestrator` | `subroutines.rs` |

---

*Last updated: 2026-06-25 — Sprint 6 complete; Phase 6 docs maintenance*
