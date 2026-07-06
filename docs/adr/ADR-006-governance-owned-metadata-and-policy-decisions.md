# ADR-006: Governance-Owned Metadata and PolicyDecisions for Core/ Governance Boundary

## Status

Accepted

## Context

Two Book I / Book II boundary issues were identified:

1. **P1-ARC-04:** `SessionRegistry` contained an `exit_counts: BTreeMap<SubRoutineHandle, u64>` field used only by `SubRoutineCleanupGuard` (Book II). The registry is a mechanism type (Book I) and should be a plain handle→session map. Per PHILOSOPHY.md §2.1, policy metadata belongs in governance-owned state, not in mechanism types.

2. **P1-ARC-03:** `StateConsistencyGuard::verify_consistency` took `&mut Session` and directly mutated `session.state`, `session.state_stack`, and `session.active_tools`. This violates the mechanism/policy boundary (PHILOSOPHY.md §2.1) and the architectural rule that governance plugins do not mutate `Session` directly (PHILOSOPHY.md §3.2, Architecture Boundaries table).

Both issues are cross-book changes: they modify the public `ConsistencyVerifier` trait (Book I) and its reference implementation (Book II), and they move state from a Book I mechanism type into a Book II governance extension state.

## Decision

### 1. Move `exit_counts` from `SessionRegistry` to a governance-owned `ExtensionStorage` state

Remove `exit_counts` from `SessionRegistry` in `brioche-core` and introduce `SubRoutineExitState` in `brioche-governance-default`:

```rust
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct SubRoutineExitState {
    pub exit_counts: BTreeMap<SubRoutineHandle, u64>,
}
```

`SubRoutineCleanupGuard::on_exit` now increments `parent.extensions.get_or_insert_default::<SubRoutineExitState>().exit_counts[handle]` before removing the child from `SessionRegistry`.

**Rationale:**
- `SessionRegistry` remains a pure mechanism type (handle→session map).
- The per-handle exit counter is governance policy metadata; storing it in `ExtensionStorage` keeps the mechanism/policy separation intact.
- `BTreeMap` preserves deterministic iteration order (I-Eco-OrderedCollections).
- `#[brioche(no_snapshot)]` matches the original in-memory semantics of `SessionRegistry` (it is not persisted across restarts).

### 2. Change `ConsistencyVerifier` to return `PolicyDecision::OverrideTransition`

Change the `ConsistencyVerifier` trait in `brioche-core` to:

```rust
pub trait ConsistencyVerifier: Send + Sync {
    fn verify_consistency(&self, session: &Session)
        -> PluginResult<Option<PolicyDecision>>;
}
```

`StateConsistencyGuard` now returns `Some(PolicyDecision::OverrideTransition(effects))` instead of mutating `session`. The kernel's `apply_consistency_check` in `finalize_transition` applies the standard recovery when it sees this variant:

```rust
PolicyDecision::OverrideTransition(verifier_effects) => {
    session.state = AgentState::Idle;
    session.state_stack.clear();
    session.active_tools.clear();
    effects.extend(verifier_effects);
}
```

Other `PolicyDecision` variants are handled by the same decision machinery used elsewhere in the kernel.

**Rationale:**
- The trait no longer exposes `&mut Session`, preventing governance code from directly mutating core state.
- The kernel owns the recovery semantics, making them uniform and predictable.
- Returning `PolicyDecision` is consistent with the existing decision vocabulary used by plugins and aggregators.
- The change is source-compatible in intent: verifiers that previously returned `Some(vec![effect])` can now return `Some(PolicyDecision::RequestEffect(effect))` or `Some(PolicyDecision::OverrideTransition(effects))` depending on whether the kernel should apply recovery.

## Consequences

### Positive

- `SessionRegistry` is a plain handle→session map; no policy state leaks into Book I mechanism types.
- `ConsistencyVerifier` cannot mutate `Session` directly; the mechanism/policy boundary is restored.
- The kernel's recovery behavior is centralized and documented.
- The public trait vocabulary is unified around `PolicyDecision`.

### Negative

- Existing `ConsistencyVerifier` implementations must update their return type from `Option<Vec<Effect>>` to `Option<PolicyDecision>`.
- The `ConsistencyVerifier` trait now takes `&Session` instead of `&mut Session`; implementations that legitimately needed to read mutable extension state must use other mechanisms (none currently do).

### Neutral

- `SubRoutineExitState` is marked `#[brioche(no_snapshot)]`, preserving the original non-persistent semantics of `exit_counts`. Future work can change this if persistence of exit counts becomes desirable.
- The `I-Gov-SubRoutineLifecycle-Guard` and `I-Comp-Rebuild-Overrides-Consistency` invariants are updated to reflect the new state location and decision type.

## Affected Invariants

| Invariant | Impact |
|-----------|--------|
| I-Gov-NoCoreMutation | **Upheld** — `ConsistencyVerifier` no longer mutates `Session`. Recovery is applied by the kernel. |
| I-Gov-SubRoutineLifecycle-Guard | **Updated** — `SubRoutineCleanupGuard` tracks exit counters in `SubRoutineExitState` (ExtensionStorage), not in `SessionRegistry`. |
| I-Comp-Rebuild-Overrides-Consistency | **Updated** — `ConsistencyVerifier` returns `PolicyDecision::OverrideTransition`; the kernel ignores it when `RebuildRoutes` is present. |
| I-Eco-OrderedCollections | **Upheld** — `SubRoutineExitState` uses `BTreeMap`. |
| I-Core-NoPanic | **Upheld** — all error paths return `Result` or `Effect`. |

## Book References

- docs/SPECS.md §Book I Ch 3.1 — `SessionRegistry`
- docs/SPECS.md §Book I Ch 3.5 — `ConsistencyVerifier` trait and `StateConsistencyGuard`
- docs/SPECS.md §Book I Ch 5.16 — `SubRoutineCleanupGuard`
- docs/SPECS.md §Book I Ch 6 — `transition()` lifecycle and fixed-trait order
- docs/architecture/book-ii-governance.md §2.3 — `ConsistencyVerifier`
- docs/architecture/book-ii-governance.md §2.8 — `SubRoutineLifecycleGuard`
- PHILOSOPHY.md §2.1 — Mechanism vs Policy in Code
- PHILOSOPHY.md §3.2 — Architecture Boundaries
- CONTRIBUTING.md §Before Submitting PR — Cross-book changes require an ADR
