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
    fn verify_consistency(&self, session: &mut Session)
        -> PluginResult<Option<Vec<Effect>>>;
}
```

Evaluated **last** in `finalize_transition`. If `Some(effects)`, the kernel applies mechanical forcing (typically `OverrideTransition` to `Idle`). Ignored if `RebuildRoutes` is present in the effects.

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
    fn drain(&self) -> Vec<EngineInput>;
}
```

**Mandatory.** Defines the invariant drain order of separate channels: `SystemSignal` → `GovernanceNotification` → `AsyncTaskResult`. Implemented by the shell (`SignalMultiplexer`), not by the kernel.


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
    fn begin_hook(&self);
    fn on_mutation(&self, type_id: TypeId, vtable: &ExtVTable, current: &dyn Any);
    fn commit_hook(&self);
    fn rollback_hook(&self, ext: &mut ExtensionStorage);
}
```

**Optional.** Provides a granular COW (Copy-On-Write) mechanism to restore the state of `ExtensionStorage` in case of budget overrun. Without injection, the kernel emits `PluginFault` without restoration.

> **Sprint 6 Note**: the mechanical integration into `ExtensionStorage` (automatic `on_mutation` call on first `get_mut`) is planned for Sprint 7. The trait and its null implementation `NoopCycleRollbackPolicy` are delivered.

**Reference implementation** : `NoopCycleRollbackPolicy` (`brioche-governance-default`).

### 2.8 SubRoutineLifecycleGuard

```rust
pub trait SubRoutineLifecycleGuard: Send + Sync {
    fn on_exit(&self, handle: SubRoutineHandle, parent: &mut Session, registry: &mut SessionRegistry)
        -> PluginResult<Vec<Effect>>;
}
```

**Mandatory.** Called by the kernel on every outgoing transition from `SubRoutine`. Without an implementation, `BriocheEngineBuilder::build()` returns `Err`.

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

1. **Inject `SessionSnapshot`** dans `ExtensionStorage`.
2. **`EpochInterceptor`** — if `Block`, immediate return.
3. **`SubRoutineHandler`** — si `SubRoutine` et `Some(effects)`, short-circuit.
4. **`on_input` hook** — pre-computed route.
5. **Dispatch principal** (`UserMessage`, `LlmStream`, `ToolCallsResult`, `RestoreSubRoutine`).
6. **`SubRoutineLifecycleGuard`** — si sortie de `SubRoutine`.
7. **`HookEffectConstraint`** — validation of emitted effects.
8. **`RebuildRoutes` position guarantee** — tronque tout ce qui suit.
9. **`ConsistencyVerifier`** — except if `RebuildRoutes` present.
10. **`GovernanceFailoverHandler`** — replaces `PluginFault` if injected.

### 3.2 Builder mandatory traits

`BriocheEngineBuilder::build()` returns `Err` if:
- `DecisionAggregator` missing
- `SubRoutineLifecycleGuard` missing

Tous les autres traits sont optionnels.

---

## Chapter 4: Default Implementations

The `brioche-governance-default` crate provides the reference implementations:

| Trait | Implementation | Fichier |
|-------|----------------|---------|
| `EpochInterceptor` | `EpochGuard` | `epoch_guard.rs` |
| `DecisionAggregator` | `LexicographicDecisionAggregator` | `policy_aggregator.rs` |
| `SubRoutineLifecycleGuard` | `SubRoutineCleanupGuard` | `subroutine_cleanup_guard.rs` |
| `ConsistencyVerifier` | `StateConsistencyGuard` | `state_consistency_guard.rs` |
| `HookEffectConstraint` | `FastHookEffectConstraint` | `hook_effect_constraint.rs` |
| `CycleRollbackPolicy` | `NoopCycleRollbackPolicy` | `noop_rollback_policy.rs` |
| `GovernanceFailoverHandler` | `SystemFailoverGuard` | `system_failover_guard.rs` |
| `SubRoutineHandler` | `SubRoutineOrchestrator` | `subroutine_orchestrator.rs` |

---

*Last updated: 2026-05-26 — Sprint 6 complete*
