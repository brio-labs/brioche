# ADR-005: `GovernanceProfile` and `BriocheEngineBuilderExt`

## Status

Accepted

## Context

`BriocheEngineBuilder` uses type-state markers to require mandatory governance traits at compile time. Constructing a working engine therefore means manually injecting:

- `DecisionAggregator` (mandatory)
- `SubRoutineLifecycleGuard` (mandatory)
- optional traits such as `EpochInterceptor`, `SubRoutineHandler`, `ConsistencyVerifier`, `HookEffectConstraint`, `CycleRollbackPolicy`, and `GovernanceFailoverHandler`

This is verbose and error-prone for applications (`agent-terminal`, `brioche-desktop`, tests). At the same time, the kernel must not depend on `brioche-governance-default`, because the default implementations crate already depends on `brioche-core`. A one-line bootstrap abstraction is needed without breaking the dependency graph.

## Decision

### 1. Define `GovernanceProfile` in `brioche-governance-default`

```rust
pub enum GovernanceProfile {
    Permissive,
    Standard,
    Strict,
}
```

Each variant encapsulates a complete, opinionated wiring of governance traits and standard plugins:

- `Permissive`: minimal policy, all effects allowed, no COW rollback.
- `Standard`: balanced policy with COW rollback, quarantine, recovery, depth guard, telemetry, and tool timeouts.
- `Strict`: maximum safeguards, tiered rollback, shorter timeouts, and comprehensive logging.

**Rationale**: Profiles are a configuration abstraction, not a kernel concept. Placing them in the default implementations crate keeps the kernel agnostic while giving applications a single decision point.

### 2. Define `BriocheEngineBuilderExt` in `brioche-governance-default`

```rust
pub trait BriocheEngineBuilderExt {
    type Output;
    fn with_profile(self, profile: GovernanceProfile) -> Self::Output;
}

impl BriocheEngineBuilderExt for BriocheEngineBuilder<Missing, Missing> {
    type Output = BriocheEngineBuilder<Present, Present>;

    fn with_profile(self, profile: GovernanceProfile) -> Self::Output {
        profile.apply(self)
    }
}
```

**Rationale**: The trait is defined in `brioche-governance-default` to avoid a circular dependency. The implementation is restricted to `BriocheEngineBuilder<Missing, Missing>` because every profile injects both mandatory traits, moving the builder into the `Present, Present` state.

### 3. Wire profiles through `GovernanceProfile::apply`

Each variant calls the builder's `with_*` methods directly:

```rust
match self {
    GovernanceProfile::Permissive => Self::apply_permissive(builder),
    GovernanceProfile::Standard => Self::apply_standard(builder),
    GovernanceProfile::Strict => Self::apply_strict(builder),
}
```

`apply_permissive`, `apply_standard`, and `apply_strict` are private functions that inject the concrete implementations (`EpochGuard`, `LexicographicDecisionAggregator`, `SubRoutineCleanupGuard`, etc.) and register standard plugins.

**Rationale**: Centralizing the wiring in one match expression makes the differences between profiles explicit and easy to audit. It also guarantees that no profile can accidentally leave a mandatory trait unset.

### 4. Keep the kernel unaware of profiles

`BriocheEngineBuilder` continues to expose individual `with_*` methods. `GovernanceProfile` is never imported by `brioche-core`. Tests and custom agents can still build engines piece by piece.

**Rationale**: Profiles are pure policy packaging. They do not add new kernel hooks, change builder invariants, or leak into the transition algorithm.

## Consequences

### Positive

- Applications bootstrap a fully wired engine with one line: `BriocheEngineBuilder::new().with_profile(GovernanceProfile::Standard).build()`.
- The dependency graph is preserved: `brioche-core` has no knowledge of `brioche-governance-default`.
- Profiles make policy differences explicit and reviewable.
- Manual builders remain available for tests and custom agents.

### Negative

- `brioche-governance-default` now owns a small piece of builder ergonomics API (`BriocheEngineBuilderExt`).
- Adding a new mandatory trait to the builder requires updating every profile variant.

### Neutral

- Profiles are additive; they do not prevent direct trait injection. A caller can apply a profile and then call additional `with_*` methods to customize.

## Affected Invariants

| Invariant | Impact |
|-----------|--------|
| I-Gov-Profile-Agnostic | **Upheld** — `brioche-core` knows nothing about `GovernanceProfile`. |
| I-Gov-TraitAtomic | **Upheld** — profiles only wire existing standalone traits; they do not introduce new trait hierarchies. |
| I-Core-Pure | **Upheld** — the kernel builder API is unchanged and performs no I/O. |

## Book References

- docs/SPECS.md §Book II Ch 9 — `brioche-governance-default`
- docs/SPECS.md §Book II Ch 6.3 — Initialization
- docs/architecture/book-ii-governance.md — Governance traits and default implementations
- PHILOSOPHY.md §2.1 — Mechanism vs Policy in Code
- CONTRIBUTING.md §Before Submitting PR — Cross-book changes require an ADR
