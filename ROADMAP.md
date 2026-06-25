# Brioche Improvement Roadmap

> This roadmap captures every improvement identified in the multi-agent audit of the Brioche workspace. Items are grouped into phases by risk/dependency: earlier phases fix correctness, security, and architectural boundaries; later phases add coverage, tooling, and polish.
>
> **Canonical tracking is now on GitHub:** see the [phase milestones](https://github.com/brio-labs/brioche/milestones) and the [roadmap issues](https://github.com/brio-labs/brioche/issues) for the live, filterable view. This file is kept as a static snapshot only.
>
> **Legend:** `P0` = critical (security/correctness/crash), `P1` = high (architecture/performance/major gaps), `P2` = medium (tests/docs/maintainability), `P3` = low (hygiene/polish). Effort: `S` small, `M` medium, `L` large, `XL` extra-large.

---

## Phase 0 — Safety & Correctness

*Goal: eliminate exploitable or correctness-breaking bugs before any other work.*

| ID | Area | Location | Problem | Action | Acceptance Criteria | Effort | Owner |
|---|---|---|---|---|---|---|---|
| P0-SEC-01 | Security | `crates/tools/brioche-tools-system/src/tools/shell.rs:124-172` | `ExecuteCommandTool` allow-lists the first whitespace token, then runs the full string via `sh -c`, allowing injection (`ls; rm -rf /`). | Parse allow-listed commands into argv and `exec` the binary directly; reject shell metacharacters. | `ls; rm -rf /` is blocked when `ls` is allowed; injection tests pass. | M | Book III-C |
| P0-SEC-02 | Security | `crates/runtime/brioche-shell-runtime/src/shell.rs:556-562` | `BriocheShell::shutdown()` is documented to drop senders but is a no-op; tasks leak. | Drop all senders, stop loops, and await tracked tasks via `TaskTracker`. | `shutdown()` returns only after engine/effect/watchdog tasks terminate; no task leaks in tests. | M | Book III-A |
| P0-SEC-03 | Security | `crates/runtime/brioche-shell-runtime/src/shell.rs:681-700` | `rebuild_in_progress` flag is set before `tokio::spawn` and cleared inside the spawned task; cancellation leaves it stuck. | Move flag set/clear into the spawned task or use a structured scope that guarantees cleanup. | Simulated cancellation mid-spawn does not leave `RebuildInProgress` true. | S | Book III-A |
| P0-SEC-04 | Security | `crates/providers/brioche-provider-openai/src/client.rs:930-953` | SSE/network errors are swallowed and `StreamEvent::Done` is never emitted on failure paths; kernel can wait forever. | Always emit `StreamEvent::Done` or a terminal `StreamEvent::Error` after `read_sse_stream`. | Mock error injection always yields a terminal stream event. | S | Book III-B |
| P0-SEC-05 | Security | `crates/runtime/brioche-shell-runtime/src/effect_executor.rs:419-438` | A fresh `CancellationToken` is created per tool call but never cancelled, so shutdown/timeout cannot interrupt tools. | Hold a shell-level `CancellationToken`, pass child tokens into tool calls, cancel on shutdown/timeout. | Tool execution stops within timeout window when shutdown is requested. | M | Book III-A |
| P0-DET-01 | Determinism | `crates/kernel/brioche-macro/src/lib.rs:418-425` | `BriocheExtensionType` generated `serialize` returns `Vec::new()` on `postcard::to_stdvec` error and persists the empty blob. | Change vtable to return `Result<Vec<u8>, _>` and surface serialization errors in `ExtensionStorage::insert`. | A serialization failure produces an error, not silent data loss; existing round-trips still pass. | M | Book I |
| P0-SEC-06 | Security | `crates/tools/brioche-tools-system/src/tools/web.rs:34-55`, `crates/providers/brioche-provider-openai/src/client.rs`, `crates/runtime/brioche-shell-persistence/src/extensions/tool_provider.rs:523-558` | `FetchUrlTool`, user `HttpPost`, and OpenAI client use default `reqwest::Client` with no timeout, URL validation, or redirect limits. | Apply `timeout_ms`, restrict schemes/hosts, cap/disable redirects, limit response size. | Wiremock tests confirm timeout, blocked `file://`/localhost, and capped redirects. | M | Books III-B/C |
| P0-SEC-07 | Security | `crates/runtime/brioche-shell-persistence/src/profiles.rs:32`, `src/settings.rs:34,101`, `src/extensions/memory_provider.rs` | API keys are written to plaintext JSON files with a misleading "encrypted at rest" comment. | Integrate OS keychain/keyring or encrypt JSON files with a user-derived key; remove misleading comment. | Secrets are no longer readable as plaintext in config files; fallback works headless. | L | Book III-A |

---

## Phase 1 — Core Kernel & Architecture

*Goal: restore Book I/II boundaries and make the hot path match its documented budget.*

| ID | Area | Location | Problem | Action | Acceptance Criteria | Effort | Owner |
|---|---|---|---|---|---|---|---|
| P1-ARC-01 | Architecture | `crates/kernel/brioche-governance/src/lib.rs` (empty), `crates/kernel/brioche-core/src/plugin.rs` | Book II crate is empty; all governance traits live in Book I Core. | Move `BriochePlugin`, `EpochInterceptor`, `DecisionAggregator`, `SubRoutineHandler`, `HookEffectConstraint`, etc. into `brioche-governance`; have `brioche-core` depend on the trait crate. | `brioche-governance` exports all policy traits; `brioche-core` compiles with only trait deps. | L | Books I/II |
| P1-ARC-02 | Architecture | `crates/kernel/brioche-core/src/plugin.rs` | `BriochePlugin` is a monolithic 8-method taxonomy trait spanning multiple lifecycle phases. | Split into atomic capability traits (`OnInput`, `OnStreamEvent`, `OnToolCalls`, `OnToolResult`, etc.) and a separate persistence capability. | Each plugin implements exactly one capability trait; routing table still pre-computes indices. | L | Book II |
| P1-PERF-01 | Performance | `crates/kernel/brioche-core/src/engine/hooks.rs:117,151,249,288`; `src/engine/dispatch.rs:79,165,235` | Every transition clones the pre-routed `Vec<usize>` route table, contradicting the "no allocation after build" claim. | Change `eval_route` and dispatch helpers to accept `&[usize]` and iterate without cloning. | No `.clone()` on route tables in hot path; benchmarks show reduced allocations. | S | Book I |
| P1-ARC-03 | Architecture | `crates/kernel/brioche-governance-default/src/guards.rs:85-109` | `StateConsistencyGuard::verify_consistency` directly mutates `session.state`, `state_stack`, and `active_tools`. | Return an `OverrideTransition(Vec<Effect>)` and let Core apply the recovery. | Governance plugin no longer takes `&mut Session`; `I-Gov-NoCoreMutation` restored. | M | Books I/II |
| P1-PERF-02 | Performance | `crates/kernel/brioche-core/src/engine.rs:206`; `src/engine/hooks.rs:69` | `SessionSnapshot` is serialized into `ExtensionStorage` at least twice per `transition()`. | Inject snapshot once at top of `transition()` and remove redundant inserts inside hooks. | Single snapshot serialization per transition; tests unchanged. | M | Book I |
| P1-ARC-04 | Architecture | `crates/kernel/brioche-core/src/types/session.rs` | `SessionRegistry` contains `exit_counts` used only by `SubRoutineCleanupGuard`. | Move exit counting into a governance-owned extension state. | `SessionRegistry` is a plain handle→session map; cleanup guard tracks its own state. | M | Books I/II |
| P1-ARC-05 | Architecture | `crates/kernel/brioche-governance-default/src/subroutines.rs` | `SubRoutineOrchestrator` reimplements user-message dispatch, tool-call accumulation, and state transitions. | Delegate to a deterministic child-step function in Core or reuse `BriocheEngine::transition()`. | No duplicated state-machine logic; child subroutine behavior matches Core. | L | Books I/II |
| P1-DET-02 | Determinism | `crates/kernel/brioche-macro/src/lib.rs:87-260` | Macro only scans top-level generic arguments; nested `Vec`/`HashMap` inside `DecisionNode`, `PolicyDecision`, `TransitionTrace` escape checks. | Add a CI-level recursive type checker or require nested carriers to derive `BriocheExtensionType`. | Nested unordered collections are caught at compile/CI time. | L | Book I |
| P1-ARC-06 | Architecture | `crates/kernel/brioche-governance-default/src/tool_pipeline.rs`, `crates/ecosystem/brioche-std/src/{gc_policy.rs,circuit_breaker.rs,pending_task_manager.rs}` | `*State` structs mirror immutable config fields. | Remove config-mirror fields; read from plugin config each hook or store config directly. | Smaller snapshots; no stale config/state divergence. | M | Books II/IV |
| P1-DET-03 | Determinism | `crates/runtime/brioche-shell-persistence/src/dto.rs`, `crates/kernel/brioche-core/src/types/session.rs`, `crates/ecosystem/brioche-std/src/{audit_logger.rs,gc_policy.rs}`, `crates/kernel/brioche-governance-default/src/tool_pipeline.rs` | Persisted/extension-serialized structs use `usize`/`isize`, creating 32-bit portability hazard. | Replace with explicit `u64`/`u32`/`i64`; cast to `usize` only at in-memory use sites. | Cross-architecture round-trip tests pass; schema is explicit. | M | Books I/III-A |

---

## Phase 2 — Shell Runtime & Async Hardening

*Goal: fix async correctness, resource leaks, and hot-path allocations outside Core.*

| ID | Area | Location | Problem | Action | Acceptance Criteria | Effort | Owner |
|---|---|---|---|---|---|---|---|
| P2-ASYNC-01 | Async | `crates/runtime/brioche-shell-runtime/src/unified_event_bus.rs:52-117` | Producer loop busy-yields and allocates a fresh `Vec` each cycle; API is unusable because receivers are consumed into the struct. | Replace `yield_now` loop with `tokio::select!` over receivers; fix `new()` vs `producer_loop` ownership. | CPU usage is near zero when idle; `producer_loop` is callable. | M | Book III-A |
| P2-ASYNC-02 | Async | `crates/runtime/brioche-shell-runtime/src/shell.rs:166-186` | `TaskTracker` appends every `JoinHandle` to an unbounded `Vec` and never prunes finished ones. | Prune finished handles in `spawn`/`health_check` or migrate to `tokio::task::JoinSet`. | Memory does not grow unbounded; `health_check` is O(active tasks). | M | Book III-A |
| P2-ASYNC-03 | Async | `crates/runtime/brioche-shell-runtime/src/effect_executor.rs:481-486` | `save_session` async mode spawns fire-and-forget with no `JoinHandle` tracking. | Return or store the `JoinHandle`; await/cancel it during shutdown. | Shutdown waits for or cleanly cancels in-flight saves. | S | Book III-A |
| P2-ASYNC-04 | Async | `crates/runtime/brioche-shell-runtime/src/engine_watchdog.rs:201-220` | After recovery trigger, watchdog loops immediately with no backoff, flooding handlers. | Add cooldown / exponential backoff or terminate after N consecutive failures. | Recovery rate is bounded; spec updated if semantics change. | S | Book III-A |
| P2-ASYNC-05 | Async | `crates/runtime/brioche-shell-projection/src/ipc_command.rs:140-143` | `load_subroutine` holds `cache.lock().await` across an `await` to storage I/O. | Reduce critical section to cache read/update only. | No lock held across await; contention reduced. | S | Book III-A |
| P2-ASYNC-06 | Async | `crates/runtime/brioche-shell-runtime/src/effect_executor.rs:145-197`, `src/llm_client.rs:60-81`, `crates/providers/brioche-provider-openai/src/client.rs:903-955` | Public async trait methods lack `# Cancel safety` documentation. | Add cancel-safety docs per `PHILOSOPHY.md` §10.1. | All `pub async fn` on `EffectExecutor`, `LlmClient`, `ToolExecutor` document cancel safety. | S | Books III-A/B/C |
| P2-ASYNC-07 | Async | `crates/providers/brioche-provider-openai/src/client.rs:113-121` | `reqwest::Client::builder().build()` errors silently fall back to `reqwest::Client::new()`. | Surface builder errors as `ShellError` or panic at startup with a clear message. | Misconfiguration is reported clearly. | S | Book III-B |
| P2-ASYNC-08 | Async | `crates/apps/agent-terminal/src/shell_builder.rs:57-75`, `crates/apps/brioche-desktop/src/commands/shell.rs:447-469` | Untracked `tokio::spawn` initialization tasks may race with the first user message. | Await initialization before returning the shell handle or expose handles. | Deterministic ordering of system prompt/schema setup vs first input. | M | Book IV |
| P2-PERF-03 | Performance | `crates/kernel/brioche-core/src/extension.rs:278-290` | `insert` serializes and writes to `cold_snapshot` even for `#[brioche(no_snapshot)]` types. | Skip `cold_snapshot` write for `NoSnapshot` strategy or add `insert_transient`. | `SignalBuffer` no longer serialized every transition; tests pass. | M | Book I |
| P2-PERF-04 | Performance | `crates/kernel/brioche-core/src/engine/finalize.rs:159-183` | `apply_governance_failover` clones every effect when any `PluginFault` is present. | Use `Vec::drain(..)` or partition in-place to avoid cloning non-fault effects. | Reduced allocations on failover paths. | M | Book I |
| P2-PERF-05 | Performance | `crates/kernel/brioche-core/src/engine/helpers.rs:206` | `append_state_effects` clones `session.active_tools` each transition. | Bench and, if needed, use `Arc<str>`/bytes for argument strings; otherwise document cost. | Clone cost documented or removed. | M | Book I |

---

## Phase 3 — Providers, Tools & Security Hardening

*Goal: harden I/O executors, error taxonomy, and default policies.*

| ID | Area | Location | Problem | Action | Acceptance Criteria | Effort | Owner |
|---|---|---|---|---|---|---|---|
| P3-SEC-01 | Security | `crates/runtime/brioche-shell-persistence/src/extensions/tool_provider.rs:446-501` | User-defined `ToolExecutor::Command` runs via `sh -c` with naive interpolation and no sandbox. | Reuse `ExecuteCommandTool` sandbox/allow-list; validate interpolated values for shell metacharacters; enforce `timeout_ms`. | User-defined command injection blocked by tests. | M | Books III-A/C |
| P3-SEC-02 | Security | `crates/apps/agent-terminal/src/shell_builder.rs:40-43` | Terminal app sets `SandboxPolicy::Permissive` with no confirmation handler. | Require `Interactive` mode or explicit opt-in flag; log all commands. | LLM cannot execute arbitrary shell without user confirmation by default. | S | Book IV |
| P3-SEC-03 | Security | `crates/tools/brioche-tools-system/src/tools/filesystem.rs:60-82` | `ReadFileTool`/`WriteFileTool`/`ListDirTool` resolve absolute/`~` paths with no base directory sandbox. | Add a configurable `base_dir` sandbox; reject paths outside the root unless explicitly allowed. | Tests confirm `~/.ssh/id_rsa` cannot be read by default. | M | Book III-C |
| P3-SEC-04 | Security | `crates/providers/brioche-provider-openai/src/client.rs:920-925` | `BRIOCHE_DIAG=1` writes full request bodies to world-readable `/tmp`. | Write to private directory (`$XDG_CACHE_HOME/brioche/diag` with 0700); redact content; truncate large bodies. | Diagnostic files are user-private and redacted. | S | Book III-B |
| P3-SEC-05 | Security | `crates/runtime/brioche-shell-persistence/src/storage.rs:140-142`, `src/dto.rs:118-157` | `SessionHeadDTO` deserialization lacks schema-version check, length limit, or snapshot validation. | Add explicit version check, max-length limit, and checksum/signature. | Corrupted or downgraded snapshots are rejected. | M | Book III-A |
| P3-SEC-06 | Security | `crates/tools/brioche-tools-system/src/registry.rs:211-222` | Tool arguments parsed as untyped `serde_json::Value`; no JSON-Schema validation. | Validate `args` against `parameters_schema()` before `run`. | Invalid types/required fields rejected before I/O. | M | Book III-C |
| P3-SEC-07 | Security | `crates/providers/brioche-provider-openai/src/sse.rs:52-56` | Malformed SSE JSON lines are silently skipped. | Count consecutive parse failures and abort after threshold. | Malicious/broken provider cannot inject infinite non-JSON events. | S | Book III-B |
| P3-SEC-08 | Security | `crates/providers/brioche-provider-openai/src/config.rs:45-47` | Empty `api_key` is accepted with default OpenAI base URL; failure only at request time. | Validate non-empty key when `base_url` requires auth; emit clear config error. | Missing key fails fast with a useful message. | S | Book III-B |
| P3-SEC-09 | Security | `crates/providers/brioche-provider-openai/src/client.rs:502-506` | HTTP error bodies are loaded without size limit. | Use `response.text().await` with a size limit or stream chunks. | Huge error responses cannot OOM the shell. | S | Book III-B |
| P3-SEC-10 | Security | `crates/runtime/brioche-shell-persistence/src/extensions/tool_provider.rs:446-457` | Template interpolation allows keys/values containing `}` to corrupt the template. | Validate placeholder keys (`^[a-zA-Z0-9_]+$`) and balanced braces. | Malformed placeholders rejected. | S | Book III-A |
| P3-SEC-11 | Security | `crates/tools/brioche-tools-system/src/registry.rs:80-92` | Default allow-list includes `cargo`, `rustc`, `git`, `find`. | Tighten defaults to read-only/safe commands; require opt-in for compilers/VCS. | Default blast radius reduced. | S | Book III-C |
| P3-SEC-12 | Security | `crates/kernel/brioche-governance-default/src/noop_traits.rs:97-137` | `Permissive` profile disables all restrictions without warning. | Document dev-only use; emit runtime warning; require confirmation in release builds. | Users are warned when selecting permissive governance. | S | Book II |
| P3-SEC-13 | Security | `crates/runtime/brioche-shell-runtime/src/telemetry.rs:75-91` | Telemetry logs arbitrary payloads without redaction. | Define a `Secret` wrapper whose `Serialize`/`Debug` redacts; audit payloads. | Secrets never emitted in telemetry. | M | Book III-A |
| P3-ERR-01 | Errors | `crates/providers/brioche-provider-openai/src/client.rs` | No `OpenAiError` type; failures collapse into opaque `ShellError::EffectExecution(String)`. | Introduce `OpenAiError` enum and a single `impl From<OpenAiError> for ShellError`. | Provider-specific context preserved; taxonomy matches `PHILOSOPHY.md` §12. | M | Book III-B |

---

## Phase 4 — CI, Tooling & Dependencies

*Goal: make CI enforce the policies it advertises and centralize dependency management.*

| ID | Area | Location | Problem | Action | Acceptance Criteria | Effort | Owner |
|---|---|---|---|---|---|---|---|
| P4-CI-01 | CI | `.github/workflows/ci.yml:125-136` | `docs` job installs toolchains but never runs `cargo doc`. | Add `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"`. | Doc warnings fail CI. | S | Repo |
| P4-CI-02 | CI | `.github/workflows/ci.yml:95-104` | `fmt-nightly` installs nightly but never runs `cargo fmt --check`. | Add `cargo +nightly-2026-06-15 fmt --check`. | Format violations fail CI. | S | Repo |
| P4-CI-03 | CI | `.github/workflows/release.yml:103` | Release build does not use `--locked`. | Add `--locked` to release build command. | Lockfile cannot drift during release. | S | Repo |
| P4-CI-04 | CI | `.github/workflows/release.yml:136-144` | Publish failures masked by `|| true`. | Remove `|| true` or use workspace publish helper. | Publish failures fail the release. | S | Repo |
| P4-CI-05 | Dependencies | Multiple `Cargo.toml` files | Crates redeclare `serde_json`, `tokio`, `reqwest` with local specs instead of `workspace = true`. | Move shared deps to workspace; use `{ workspace = true, features = [...] }`. | Single source of truth for versions/features. | M | Repo |
| P4-CI-06 | Dependencies | `deny.toml:63,73-78` | `unknown-registry`/`unknown-git` warn; `wildcards = "allow"`. | Set to `deny`/`warn`; restrict wildcard requirements. | Malicious sources and wildcard direct deps blocked. | S | Repo |
| P4-CI-07 | CI | `.github/workflows/ci.yml:47-58` | `deny.toml` changes do not trigger the deny job. | Add `deny.toml` to the `lock` path filter. | Policy changes must pass `cargo deny`. | S | Repo |
| P4-CI-08 | CI | `.github/workflows/ci.yml:89`, `release.yml:49`, `scripts/pre-commit.sh:20` | Inconsistent `cargo deny` invocation. | Standardize on `cargo deny check all`. | Same command used everywhere. | S | Repo |
| P4-CI-09 | Linting | `clippy.toml:2` | `cognitive-complexity-threshold = 30` but lint is not enabled. | Promote `clippy::cognitive_complexity` to `warn` in workspace lints; later `deny`. | Complexity warnings surface in CI. | S | Repo |
| P4-CI-10 | Dependencies | `Cargo.lock` | Duplicate transitive crates (`syn`, `indexmap`, `hashbrown`, `bitflags`, `windows-sys`) not surfaced. | Add `cargo tree --duplicates` step to dependency-audit job. | Duplicates visible in CI logs. | S | Repo |
| P4-CI-11 | Dependencies | `crates/apps/brioche-desktop/Cargo.toml:25-28` | Tauri runtime/plugins versions misaligned. | Move Tauri deps to workspace with aligned minors; review advisory ignores quarterly. | Predictable Tauri updates; fewer ignored advisories. | M | Repo |
| P4-CI-12 | Tooling | `scripts/pre-commit.sh:34-37` | `grep -rq "unwrap()\|expect("` uses broken basic regex. | Replace with `grep -rEq "unwrap\(\)|expect\("` or remove redundant check. | Pre-commit actually catches unwrap/expect if kept. | S | Repo |
| P4-CI-13 | CI | `.github/workflows/conventional-commits.yml:21,37` | PR title/commit messages interpolated into shell commands unquoted. | Use environment variables and quote expansions. | No shell injection from PR metadata. | S | Repo |
| P4-CI-14 | Tooling | `scripts/check-invariants.sh` | Script is not wired into CI or pre-commit; may drift. | Invoke it from CI/pre-commit or delete in favor of `cargo deny check all`. | Script is either exercised or removed. | S | Repo |
| P4-CI-15 | Linting | `crates/kernel/brioche-macro/Cargo.toml` | Macro crate duplicates workspace lint config. | Use `[lints] workspace = true` and only override if necessary. | Config cannot drift. | S | Book I |

---

## Phase 5 — Test Coverage

*Goal: close the gaps identified in the Testing Canon (`PHILOSOPHY.md` §9).* 

| ID | Area | Location | Problem | Action | Acceptance Criteria | Effort | Owner |
|---|---|---|---|---|---|---|---|
| P5-TEST-01 | Tests | `tests/{integration,property,replay,bench}/README.md` | Workspace test directories contain only stubs. | Add cross-crate property, replay, integration, and benchmark suites. | All four directories contain exercising `.rs` files. | L | All |
| P5-TEST-02 | Tests | `crates/kernel/brioche-core/tests/property_transition.rs` | No property tests for `AgentState` sequences. | Add `proptest` for push/pop/stack depth/generation_id monotonicity/`Failure` rejection. | Invariants hold across 10k+ random sequences. | M | Book I |
| P5-TEST-03 | Tests | `crates/runtime/brioche-shell-runtime/tests/replay_journal.rs` | Replay coverage limited to simple `UserMessage`/`Done` sequences. | Add tool-call, subroutine restore, epoch mismatch, plugin fault, and wraparound scenarios. | Complex replays match original effect/state sequences. | M | Book III-A |
| P5-TEST-04 | Tests | `crates/runtime/brioche-shell-runtime/tests/effect_executor_tests.rs` | Only `NoopPersistence` tested; `DefaultEffectExecutor` variants not exercised. | Add tests using `MockLlmClient`/`EchoToolExecutor` for each `Effect` variant. | Each effect variant produces expected side effect. | M | Book III-A |
| P5-TEST-05 | Tests | `crates/kernel/brioche-governance-default/src/{subroutines,input_guards,error_safety,timeouts,negotiation_broker,rollback}.rs` | Governance plugins wired into profiles have no dedicated tests. | Add unit tests for each plugin's core behavior. | All listed modules have direct test coverage. | L | Book II |
| P5-TEST-06 | Tests | `crates/kernel/brioche-core/src/engine.rs:302,318,336`; `src/extension.rs:424,435,447` | Public methods `rebuild_routes`, `remove_subroutine`, rollback policy attach/detach untested. | Add direct tests in `engine_transition.rs` and `extension_storage.rs`. | Methods behave as documented. | M | Book I |
| P5-TEST-07 | Tests | `crates/kernel/brioche-macro/tests/compile_test.rs` | No negative compilation tests for `#[brioche_plugin]`/`#[brioche_offload_task]`. | Add `trybuild` fail/pass cases. | Macros reject invalid inputs; accept valid ones. | M | Book I |
| P5-TEST-08 | Tests | `crates/kernel/brioche-macro/tests/ui/` | `BriocheExtensionType` derive tests miss enums, generics, `ext_id`, incremental snapshots, nested deterministic `Vec`, aliased `HashMap`. | Add corresponding pass/fail UI tests. | Full macro coverage. | M | Book I |
| P5-TEST-09 | Tests | `crates/kernel/brioche-core/tests/session_types.rs` | `HistoryEdit::Replace`/`Truncate` and out-of-bounds cases untested. | Add edge-case tests. | All edit variants and errors covered. | S | Book I |
| P5-TEST-10 | Tests | `crates/kernel/brioche-core/tests/extension_storage.rs`, `crates/runtime/brioche-shell-persistence/tests/persistence_tests.rs` | Determinism of serialization not asserted across MessagePack/postcard. | Add proptests asserting bit-for-bit identical output for identical input. | Reordered collections produce canonical bytes. | M | Books I/III-A |
| P5-TEST-11 | Tests | `crates/kernel/brioche-core/tests/engine_transition.rs`, `crates/runtime/brioche-shell-runtime/tests/shell_runtime.rs` | Tests assert current behavior or just liveness instead of invariants. | Refactor to assert post-conditions (state, history, generation_id, effects). | Tests verify invariants, not just presence. | M | Books I/III-A |
| P5-TEST-12 | Tests | `crates/kernel/brioche-core/tests/engine_transition.rs` | Core tests use mocks for mandatory traits instead of production profiles. | Add parallel production-wiring tests with `GovernanceProfile::Standard`/`Strict`. | Real aggregator/lifecycle behavior exercised. | M | Book I |
| P5-TEST-13 | Tests | `crates/runtime/brioche-shell-runtime/`, `crates/providers/brioche-provider-openai/` | No benchmarks for shell effect-loop throughput or OpenAI SSE parsing. | Add Criterion/Divan benches for effect loop, backpressure, SSE parsing. | Regression benchmarks exist. | M | Books III-A/B |
| P5-TEST-14 | Tests | `crates/providers/brioche-provider-openai/tests/mock_server.rs` | Missing malformed SSE, partial fragments, timeout, retry tests. | Add wiremock error scenarios. | Provider error paths covered. | M | Book III-B |
| P5-TEST-15 | Tests | `crates/apps/brioche-desktop/src/commands/session.rs`, `src/commands/shell.rs` | Desktop app has minimal tests and hard-coded `/tmp` paths. | Add focused command tests using `tempfile`; cover error paths. | Desktop commands tested without Tauri runtime. | M | Book IV |

---

## Phase 6 — Documentation & Maintainability

*Goal: eliminate stale docs, standardize contracts, and remove duplication.*

| ID | Area | Location | Problem | Action | Acceptance Criteria | Effort | Owner |
|---|---|---|---|---|---|---|---|
| P6-DOC-01 | Docs | `crates/kernel/brioche-core/src/engine/{dispatch,helpers,hooks,router,trace}.rs`, `src/types/*.rs` | Hot-path functions embed `Complexity:` in summary sentences. | Convert all to `# Complexity` headings; add missing `# Panics` sections. | All hot-path items use standardized doc structure. | M | Book I |
| P6-DOC-02 | Docs | `crates/kernel/brioche-core/src/plugin.rs`, `src/extension.rs` | Trait methods and type aliases have placeholder docs without `Refs:`/`# Complexity`/`# Panics`. | Backfill full contract docs for every public trait item. | `cargo doc` with `-D warnings` passes; all pub items fully documented. | M | Book I |
| P6-DOC-03 | Docs | `docs/architecture/README.md`, `book-i-core.md`, `book-ii-governance.md` | Index lists missing files; spec references stale filenames; French prose present. | Prune or create missing specs; update filenames; translate to English. | Architecture docs match source tree. | M | Repo |
| P6-DOC-04 | Docs | `CONTRIBUTING.md` "Before Submitting PR" | Claims lint tool is unimplemented and uses inconsistent command names. | Update guidance; standardize on actual binary name. | Contributors run the real tool. | S | Repo |
| P6-DOC-05 | Docs | `scripts/philosophy-check.py` | Accepts `Refs: SPECS` as valid, enabling malformed refs. | Tighten validation to require `Refs: I-Category-Name` or documented path format. | Malformed refs fail CI. | S | Repo |
| P6-DOC-06 | Docs | `docs/adr/` | Missing ADRs for `SubRoutineHydrator`, `SignalDrainOrder`/`SignalBuffer`, `GovernanceProfile`/`BriocheEngineBuilderExt`. | Add ADRs for recent cross-book abstractions. | Cross-book changes documented. | M | Repo |
| P6-MAINT-01 | Maintainability | `crates/kernel/brioche-governance/src/lib.rs`, `crates/kernel/brioche-macro/Cargo.toml`, `docs/first-pr-guide.md:59` | Empty crate with stale references and unused dependency. | Move traits in or delete crate and update all references. | Crate has a clear purpose or is removed. | L | Books I/II |
| P6-MAINT-02 | Maintainability | `crates/ecosystem/brioche-playground/src/lib.rs` | Empty library file; crate is binary-only. | Make `[[bin]]`-only or merge docs into `main.rs` and remove `lib.rs`. | No empty lib.rs. | S | Book IV |
| P6-MAINT-03 | Maintainability | `crates/apps/agent-terminal/src/shell_builder.rs`, `crates/apps/brioche-desktop/src/commands/shell.rs`, `crates/runtime/brioche-shell-persistence/src/extensions/*.rs` | Duplicated `system_time_secs`, JSON load/save, OpenAI config assembly, `build_shell` routines. | Extract shared helpers into `brioche-shell-runtime` or a new `brioche-shell-builder` crate. | Common routines live in one place. | L | Books III-A/IV |
| P6-MAINT-04 | Maintainability | `crates/runtime/brioche-shell-persistence/src/extensions/{memory_provider,skill_provider,tool_provider}.rs` | Extension traits return `Result<T, String>`. | Define `PersistenceError` enum and use it across traits; map at boundary. | No `String` errors in public trait APIs. | M | Book III-A |
| P6-MAINT-05 | Maintainability | `crates/kernel/brioche-core/src/engine/helpers.rs` | Generic name for real transition mechanics file. | Rename to `transition_support.rs` or `transition_steps.rs`. | Filename reflects contents. | S | Book I |
| P6-MAINT-06 | Maintainability | `crates/infra/cargo-brioche-lint/src/main.rs`, `crates/infra/cargo-brioche-lint-invariants/src/main.rs` | Duplicate directory walking, CLI parsing, reporting code. | Extract `brioche-lint-core` library or merge into subcommands. | M | Repo |
| P6-MAINT-07 | Maintainability | `.github/CODEOWNERS:6` | Bare `TODO` placeholder. | Remove or convert to issue. | No bare TODOs. | S | Repo |
| P6-MAINT-08 | Maintainability | `crates/kernel/brioche-governance-default/src/*.rs`, `crates/ecosystem/brioche-std/src/*.rs` | Magic priority literals scattered everywhere. | Introduce named constants or enum (e.g., `Priority::Epoch`, `Priority::Quarantine`). | Ordering policy is explicit. | S | Books II/IV |

---

## Healthy Areas to Preserve

The following areas are already in good shape and should not be regressed:

- **Core panic discipline**: no `unwrap`/`expect`/`panic` in `brioche-core`/`brioche-governance` production code.
- **Effect-permission model**: explicit `EffectBit` bitmask with `FastHookEffectConstraint` O(1) validation.
- **Determinism guard**: `BTreeMap`/`BTreeSet` in persisted state; `HashMap`/`HashSet`/`Instant::now` banned in Core.
- **Bounded channels and backpressure**: primary `mpsc` channels are bounded with documented capacities.
- **Error taxonomy at boundaries**: `BriocheError` stays in Core; governance uses `PluginError`; shell uses `ShellError`.
- **Lock-free transition journal**: `TransitionJournal` soundness and documentation are good.
- **ADRs for major cross-book seams**: rebuild routes, settings-driven registry, `SubRoutineHydrator`.

---

## Done

| ID | PR / Notes |
|---|---|
| P4-CI-01 | #59: `docs` CI job now runs `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"`. |
| P4-CI-02 | #59: `fmt-nightly` CI job now runs `cargo +nightly-2026-06-15 fmt --check`. |
| P4-CI-03 | #59: Release build now uses `--locked` to prevent lockfile drift. |
| P4-CI-04 | #59: Crate publish step no longer masks failures with `|| true`. |
| P4-CI-08 | #59: All `cargo deny` invocations standardized to `cargo deny check all`. |
| P6-DOC-05 | #59: `scripts/philosophy-check.py` no longer accepts malformed `Refs: SPECS`; fixed the one occurrence in `brioche-core/src/engine/builder.rs`. |
| P6-DOC-01 | #149: Converted embedded `Complexity:` in Book I Core docs to `# Complexity` headings; added missing `# Panics` sections. |
| P6-DOC-02 | #151: Backfilled full contract docs (`Refs:`, `# Complexity`, `# Panics`) for `BriochePlugin` required methods, governance trait methods, and extension type aliases. |
| P6-DOC-04 | #149: Fixed `CONTRIBUTING.md` lint-invariants guidance to use correct subcommands and removed unimplemented note. |
| P6-MAINT-02 | #147: Removed empty `lib.rs` from `brioche-playground`; crate is binary-only. |
| P6-MAINT-05 | #147: Renamed `crates/kernel/brioche-core/src/engine/helpers.rs` to `transition_support.rs`. |
| P6-MAINT-07 | #147: Removed bare `TODO` placeholder from `.github/CODEOWNERS`. |
| P6-MAINT-08 | #151: Replaced magic `priority()` literals with named `Priority` constants in `brioche-governance-default` and `brioche-std`. |
| P3-SEC-07 | #155: `SseParser` now aborts after a configurable threshold of consecutive malformed `data:` lines; `OpenAiLlmClient` propagates the abort as a terminal error. |
| P6-DOC-03 | #156: Pruned architecture index, fixed stale filenames, translated French prose, aligned governance trait signatures. |
| P6-DOC-06 | #156: Added ADR-004 (`SignalDrainOrder`/`SignalBuffer`) and ADR-005 (`GovernanceProfile`/`BriocheEngineBuilderExt`). |
| P1-PERF-01 | #157: `eval_route` and dispatch helpers now accept `&[usize]` and iterate without cloning route tables. |
| P1-PERF-02 | #157: `SessionSnapshot` injected once at top of `transition()`; redundant hook inserts removed. |
| P5-TEST-01 | #166: Populated `tests/{integration,property,replay,bench}/` with exercising `.rs` suites. |
| P5-TEST-02 | #169: Added `proptest` for `AgentState` sequences (push/pop, stack depth, generation_id monotonicity, Failure rejection). |
| P4-CI-07 | #154: `deny.toml` added to CI `lock` path filter so policy changes trigger the deny job. |
| P4-CI-12 | #154: Fixed broken unwrap/expect regex in `scripts/pre-commit.sh`. |
| P4-CI-13 | #154: Quoted PR metadata in `.github/workflows/conventional-commits.yml` to prevent shell injection. |
| P4-CI-14 | #154: Removed unused `scripts/check-invariants.sh`. |
| P4-CI-15 | #154: `crates/kernel/brioche-macro/Cargo.toml` now uses `[lints] workspace = true`. |
| P4-CI-09 | #170: `clippy::cognitive_complexity` promoted to `warn` in workspace lints. |
| P4-CI-10 | #170: `cargo tree --duplicates` added to dependency-audit CI job. |
| P2-PERF-03 | #167: `ExtensionStorage::insert` skips `cold_snapshot` for `NoSnapshot` types. |
| P2-PERF-04 | #167: `apply_governance_failover` avoids cloning non-fault effects via in-place partitioning. |
| P2-PERF-05 | #167: `append_state_effects` clone cost removed/documented; argument strings use `Arc\u003cstr\u003e`. |
| P5-TEST-03 | #168: Extended `replay_journal.rs` with tool-call, subroutine restore, epoch mismatch, plugin fault, and wraparound scenarios. |
| P5-TEST-04 | #168: Rewrote `effect_executor_tests.rs` to exercise every `Effect` variant via `MockLlmClient`/`EchoToolExecutor`. |

---

## How to Use This Roadmap

1. **Start at Phase 0** and work downward; items within a phase can often be done in parallel.
2. **Create one branch per item** following the branch naming convention in `CONTRIBUTING.md`.
3. **Update this file** when an item is completed (change status/assignee, or move to a "Done" section).
4. **Add ADRs** for any Phase 1 architectural change that crosses Book boundaries.
5. **Run the verification command** for each Book before submitting:
   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo deny check all
   cargo test --workspace
   cargo doc --workspace --no-deps --all-features
   ```
