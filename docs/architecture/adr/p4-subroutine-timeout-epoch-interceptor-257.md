# ADR: Sub-routine timeout evaluation before delegation for #257

## Status

Accepted

## Context

Issue #257 identified a cross-book failure between Book I (Core transition
ordering) and Book II (Governance timeout policy): `SubRoutineTimeoutPolicy`
ran only as an `OnInput` hook on the parent session.

`BriocheEngine::transition()` evaluates sub-routine delegation before parent
`on_input`. While a parent is in `AgentState::SubRoutine`, successful child
processing short-circuits parent dispatch so the same input is not processed
twice. That short-circuit also skipped parent `on_input`, so active child
traffic could keep flowing while the parent-level sub-routine timer never ran.

The invariant already requires epoch interception before sub-routine handling
(`I-Comp-Epoch-Subroutine`). Timeout expiry is also a pre-delegation barrier:
once a timer is expired, the child must not process another input.

## Decisions

1. **Allow multiple epoch interceptors.**
   - `BriocheEngineBuilder::with_epoch_interceptor` now appends to an ordered
     chain instead of replacing one optional slot.
   - `BriocheEngine::transition()` runs the chain in builder-registration order.
   - The first `EpochAction::Block` returns immediately, preserving the existing
     barrier semantics.

2. **Move sub-routine timeout checks onto the epoch-interceptor path.**
   - `SubRoutineTimeoutPolicy` implements `EpochInterceptor`.
   - The existing `on_input` hook delegates to the same evaluation function for
     compatibility with non-delegated transitions.
   - `Standard` and `Strict` governance profiles register `EpochGuard` first and
     `SubRoutineTimeoutPolicy` second, so stale-generation rejection remains the
     earliest barrier.

3. **Keep policy out of Core.**
   - Core only composes a trait chain and maps `Block` to existing effects.
   - Timeout-specific state, tick interpretation, and expiry decisions remain in
     `brioche-governance-default`.

4. **Cover the active-delegation regression.**
   - The regression test constructs a parent in `AgentState::SubRoutine`, an
     active child session, and an expired timer.
   - The transition returns before child delegation and leaves the child
     registered.

## Consequences

- Independent pre-delegation barriers can compose without wrapper traits or
  Core business-rule branches.
- Existing single-interceptor callers keep working; each call appends one
  interceptor.
- Governance profiles no longer rely on parent `on_input` to enforce
  sub-routine timeouts during active child execution.
- The `EpochAction::Block` effect mapping remains the existing
  `ErrorCode::EpochMismatch` path even when the blocking interceptor is a
  timeout policy. A later error-taxonomy change can split barrier reasons if the
  UI needs distinct codes.

## Invariants

Refs: I-Comp-Epoch-First, I-Comp-Epoch-Subroutine,
I-Gov-SubRoutineLifecycle-Guard, I-Gov-NoCoreMutation
