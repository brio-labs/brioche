# ADR: Shell API surface and contract-suite cohesion for issues #282 and #286

## Status

Accepted

## Context

Issues #282 and #286 crossed the shell and desktop boundary without changing Core
mechanism behavior:

- **Book III-A — Shell Runtime** owned large integration suites that mixed shell
  construction, dispatch, effect execution, signals, telemetry, persistence, and
  recovery contracts in one file.
- **Book III-B — Shell Persistence** owned a large integration suite that mixed
  DTO conversion, serialization, Redb storage, delta persistence, cache,
  hydration, GC, and determinism contracts in one file.
- **Book IV — Desktop** exposed session manager and factory internals broadly,
  leaving multi-field lifecycle invariants to caller discipline.

The change is cross-book because the review shape spans runtime tests,
persistence tests, desktop shell construction, and persisted tool descriptors.
The desired outcome is tighter contracts and easier review, not a new runtime
abstraction layer.

## Decisions

1. **Split integration tests by observable contract.**
   - Runtime suites now follow shell construction/dispatch, backpressure,
     effect execution, signals/events, watchdog/telemetry, persistence modes,
     and recovery/fault routing.
   - Persistence suites now follow DTO conversion, serialization format,
     delta storage, subroutine hydration, GC/recovery, and determinism.
   - Shared test support is limited to helpers reused by multiple focused
     suites.

2. **Keep replay journal tests cohesive.**
   - `replay_journal.rs` stays as one replay-contract suite because its cases
     share setup and remain below the philosophy test-fixture threshold.
   - Sprint chronology labels were removed so failures point to the invariant
     owner instead of process history.

3. **Narrow desktop mutable API surface without OOP wrappers.**
   - `DesktopState` exposes intent-named helpers for settings replacement,
     factory snapshots, extension snapshots, current-session id lookup, and
     footer context note access.
   - `ShellFactory` is built through a constructor from typed dependencies.
   - Session lifecycle mutation stays on `SessionManager` methods so session and
     metadata updates remain local to the lifecycle owner.

4. **Constrain tool descriptor ownership.**
   - `ToolDescriptor.source` is a typed `ToolSource` enum with stable serialized
     frontend values.
   - `UserToolDefinition.parameters` remains arbitrary JSON because user tools
     are the explicit extension boundary.

## Consequences

- Test failures now identify the broken runtime or persistence contract before a
  reviewer opens the file.
- No runtime dispatch, Core transition, or governance policy behavior changes.
- Desktop command code still uses explicit shell/app effects; policy remains out
  of Core.
- Tool source ownership is exhaustively typed while preserving the existing IPC
  string contract.

## Invariants

Refs: I-Shell-Runtime-OnlyIO
