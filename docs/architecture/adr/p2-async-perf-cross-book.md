# ADR: Cross-book async hardening for P2-ASYNC / P2-PERF issues #78–#87

## Status

Accepted

## Context

The Phase 2 Shell Runtime & Async Hardening issues (#78–#87) span multiple
architectural books:

- **Book III-A — Shell Runtime**: effect execution, task tracking, engine
  watchdog, shell lifecycle.
- **Book III-B — Providers**: OpenAI LLM client initialization and error
  propagation.
- **Book III-C — Tools**: system tool executor registration and schema setup.
- **Book IV — Apps**: `agent-terminal` and `brioche-desktop` shell builders.

Because the fixes require coordinated changes across these boundaries, this ADR
records the decisions made and the invariants they uphold.

## Decisions

1. **`ShellBuilder::build` is `async` and returns `Result`.**
   - The default system prompt and tool schemas are pushed into the LLM client
     *before* the function returns, eliminating the race where the first user
     message could be processed before initialization finished.
   - The builder no longer spawns fire-and-forget tasks for initialization.

2. **`OpenAiLlmClient::new` returns `Result`.**
   - `reqwest::Client::builder().build()` errors are surfaced as
     `OpenAiError::HttpClientBuilder` instead of silently falling back to
     `reqwest::Client::new()`.
   - Provider errors are mapped to `ShellError` at the boundary and never leak
     into `Effect` payloads.

3. **Async persistence is tracked and shut down gracefully.**
   - `DefaultEffectExecutor` stores `JoinHandle`s for async `save_session` tasks.
   - `EffectExecutor::shutdown` drains and awaits those handles before the
     runtime exits.

4. **The `load_subroutine` cache lock is never held across I/O.**
   - The lock is acquired only for short cache reads/updates.
   - `RedbStorage::load_session` runs outside the lock.

5. **The engine watchdog uses exponential backoff.**
   - After each consecutive missed pong, the next ping is delayed by
     `100 ms * 2^(n-1)`.
   - The delay is capped at `max(max_response_delay_ms, heartbeat_interval_ms * 8)`.
   - The counter resets as soon as a pong is received.

6. **`TaskTracker` prunes finished handles.**
   - `spawn` and `health_check` remove completed handles to prevent unbounded
     growth.

7. **All `pub async fn` document their `# Cancel safety` contract.**
   - `LlmClient`, `ToolExecutor::execute`, and `OpenAiLlmClient` implementations
     now include explicit cancel-safety notes.

## Consequences

- Callers in app crates must `await` `build_shell` and handle the returned
  `ShellError`.
- `agent-terminal` and `brioche-desktop` propagate initialization failures to
  the user instead of silently using a half-initialized shell.
- Background persistence tasks are awaited at shutdown, removing the risk of
  detached saves.
- Cache contention and watchdog tight-looping are reduced.

## Invariants

Refs: I-Shell-Runtime-OnlyIO, I-Shell-Persistence-Mode, I-Shell-Load-Batch,
I-Shell-Watchdog-NoKill, I-Shell-Network-Signal, I-Core-ChunkBudget,
I-Shell-ToolResult-PassThrough
