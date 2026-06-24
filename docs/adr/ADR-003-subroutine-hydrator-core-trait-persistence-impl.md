# ADR-003: `SubRoutineHydrator` Trait in Core, Implementation in Shell Persistence

## Status

Accepted

## Context

`EngineInput::RestoreSubRoutine` carries a serialized session head blob that the kernel must decode into a live `Session` before inserting it into the `SessionRegistry`. The blob is produced by `brioche-shell-projection` (MessagePack-encoded `SessionHeadDTO`) and the decoder lives in `brioche-shell-persistence`.

This creates a boundary problem:

- `brioche-core` cannot depend on `brioche-shell-persistence` without inverting the architectural dependency graph (persistence already depends on core).
- The kernel cannot remain agnostic of the encoding while still performing the restore.
- A full `SessionHeadDTO` deserialization and message-history replay is a Sprint 5+ feature, but v0.1 needs a safe placeholder that at least uses the decoded head instead of discarding it.

## Decision

### 1. Define a boundary trait `SubRoutineHydrator` in `brioche-core`

The trait has a single method:

```rust
pub trait SubRoutineHydrator: Send + Sync {
    fn hydrate(&self, head_blob: &[u8]) -> Result<Session, BriocheError>;
}
```

It is injected through `BriocheEngineBuilder::with_subroutine_hydrator` and stored in `GovernanceKernel`. `dispatch_restore_subroutine` calls it when present; on failure it emits `Effect::Error(StateInconsistency/TransitionFailed)` and falls back to a blank `Session::new(handle)`.

**Rationale**: The kernel owns the restore transition and the `SessionRegistry`, but it should not know the persistence encoding. A trait defines the minimal capability the kernel needs (blob -> Session) and lets the persistence layer supply the implementation.

### 2. Implement the trait in `brioche-shell-persistence`

`PersistenceSubRoutineHydrator` uses the existing `deserialize_head` (MessagePack) and `SessionHeadDTO::to_session` helpers. For the v0.1 placeholder it restores the head with an empty message history; full message replay is deferred to a later sprint.

**Rationale**: Persistence owns the DTO schema and the decoder. Keeping the implementation there preserves the dependency direction (persistence depends on core, not the reverse) and lets the DTO evolve without touching core.

### 3. Wire the implementation in app-level engine builders

`agent-terminal` and `brioche-desktop` build the `BriocheEngine` via `PluginBuilder::standard()` in `brioche-plugin-kit`. The standard builder now injects `PersistenceSubRoutineHydrator` by default.

**Rationale**: The kernel is constructed at the application layer, which is the only place that knows both the engine and the persistence backend. This avoids leaking persistence details into the runtime crate while giving every desktop/terminal shell the wired implementation by default.

### 4. Keep the placeholder semantics explicit

- The `head_blob` doc comment in `Effect::RestoreSubRoutine` is corrected from "postcard-encoded" to "MessagePack-encoded".
- The fallback path on deserialization failure is preserved and observable via `Effect::Error`.
- Message history is not restored in v0.1; the DTO carries a `persisted_msg_count` watermark, but the actual messages are replayed separately in future work.

**Rationale**: Honest placeholders are safer than silent stubs. The code documents what is and is not implemented, and the shell can surface failures instead of silently creating blank sub-routines.

## Consequences

### Positive

- Core remains independent of persistence encoding.
- The dependency graph is preserved: `brioche-core` <- `brioche-shell-persistence`.
- Sub-routine restore now uses the persisted head (ID, state, state stack) instead of discarding it.
- Failure is observable through typed effects rather than silent fallback.

### Negative

- `BriocheEngineBuilder` gains another optional injection point.
- Apps that build engines manually must remember to wire a hydrator if they want restore to use persisted state.
- Message history is not restored; a sub-routine revived from disk will have an empty `history` until the full replay mechanism is implemented.

### Neutral

- The trait is a v0.1 boundary seam. Future work (full history replay, sub-routine cache warming) will extend or replace the hydrate contract rather than add new ad-hoc decoders.

## Affected Invariants

| Invariant | Impact |
|-----------|--------|
| I-Shell-Session-NoSend | **Upheld** — `Session` is still constructed and owned on the engine thread. |
| I-Persist-Idempotence | **Upheld** — DTO round-trip is preserved; restore is deterministic given the same blob. |
| I-Core-Pure | **Upheld** — Core performs no I/O and has no knowledge of MessagePack or Redb. |
| I-Shell-DTO-Only | **Upheld** — Only the persistence layer manipulates `SessionHeadDTO`. |

## Book References

- docs/SPECS.md §Book I Ch 6 — `EngineInput` dispatch and `SessionRegistry`
- docs/SPECS.md §Book III-A Ch 1 — Shell startup and engine construction
- docs/SPECS.md §Book III-B Ch 1.1 — `SessionHeadDTO` as the persistence unit
- PHILOSOPHY.md §2.1 — Mechanism vs Policy in Code
- PHILOSOPHY.md §4.4 — Architecture Decision Records in Code
- CONTRIBUTING.md §Before Submitting PR — Cross-book changes require an ADR
