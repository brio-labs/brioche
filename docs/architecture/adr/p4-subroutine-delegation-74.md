# ADR: Delegate SubRoutine Transitions to Core Engine

## Context

Issue #74 identified that `SubRoutineOrchestrator` in `brioche-governance-default` was reimplementing core state-machine logic. Specifically, it duplicated user-message dispatch, tool-call accumulation, and state transitions that already exist in `brioche-core`'s `BriocheEngine`. This duplication creates a maintenance burden, increases the risk of subtle bugs where the orchestrator's state transitions diverge from the core engine's rules, and violates the principle that mechanism (state machines) belongs in Core while policy belongs in Governance.

## Decision

We have modified the core engine's `apply_subroutine_handler` to execute the child session's transition natively via `self.transition(&mut child, input)` *before* yielding control to the `SubRoutineHandler` governance trait.

As a result, `SubRoutineOrchestrator` is stripped of all transition logic and now solely implements policy: monitoring the child session's state to detect termination (`Idle` or `Failure`) and bubbling those effects up to the parent session.

## Consequences

- **Mechanism/Policy Separation**: State transitions are strictly managed by `brioche-core`, adhering to our core philosophy.
- **Code Reduction**: Redundant helper functions (`delegate_user_message`, `accumulate_stream_tools`, `resolve_tool_results`) have been eliminated from `brioche-governance-default`.
- **Borrow Checker**: We restructured `apply_subroutine_handler` to release the mutable borrow on `self` during the child transition, resolving overlapping borrow issues.
- **Cross-Book Change**: This change touched `brioche-core` (Book I) and `brioche-governance-default` (Book II), moving mechanism code out of Book II and into Book I.

## Invariants Affected

- `I-Comp-Epoch-Subroutine`
- `I-Shell-Session-NoSend`
