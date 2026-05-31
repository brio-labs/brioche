# Complete Local Changes Report — Brioche PR #25

> **Generated:** 2026-05-31  
> **Branch:** `feat/typed-ui-widgets-cli-provider-tools`  
> **Files changed:** 68 files (+5,648 / −689 lines)  
> **Modified:** 43 files  
> **Deleted:** 1 file (`crates/brioche-std/src/tool_result_policy.rs`)  
> **New crates:** 3 (`brioche-cli`, `brioche-provider-openai`, `brioche-tools-system`)

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [New Crates (Complete File-by-File)](#2-new-crates-complete-file-by-file)
3. [Core Changes (`brioche-core`)](#3-core-changes-brioche-core)
4. [Shell Runtime Changes (`brioche-shell-runtime`)](#4-shell-runtime-changes-brioche-shell-runtime)
5. [Shell Projection Changes (`brioche-shell-projection`)](#5-shell-projection-changes-brioche-shell-projection)
6. [Shell Persistence Changes (`brioche-shell-persistence`)](#6-shell-persistence-changes-brioche-shell-persistence)
7. [Governance Changes (`brioche-governance-default`)](#7-governance-changes-brioche-governance-default)
8. [Standard Library Changes (`brioche-std`)](#8-standard-library-changes-brioche-std)
9. [Plugin Kit & Macro Changes](#9-plugin-kit--macro-changes)
10. [Workspace & Dependency Changes](#10-workspace--dependency-changes)
11. [Documentation Changes](#11-documentation-changes)
12. [PR Checklist & Breaking Changes](#12-pr-checklist--breaking-changes)

---

## 1. Executive Summary

This branch introduces **five major architectural themes** that transform Brioche from a kernel/SDK into a runnable end-user product:

### Theme 1: Typed UI Effects (`UiWidget` enum)
Replaces the `Effect::ForwardToUi { widget_type: String, payload: serde_json::Value }` stringly-typed anti-pattern with an exhaustive `UiWidget` enum across **all layers** — core, projection, governance, and SPECS. This is a **breaking change** for any downstream code constructing raw `ForwardToUi` effects.

### Theme 2: LLM Streaming Text Buffering
Adds `Session.pending_assistant_text: String` to the kernel. Text chunks arriving via `StreamEvent::TextChunk` are accumulated during streaming and materialized as `ChatMessage::Assistant` at `StreamEvent::Done` or `StreamEvent::ToolCallDone`. Previously, only tool calls were captured; assistant text was lost.

### Theme 3: Three New End-User Crates
- **`brioche-cli`** — Full terminal REPL (reedline + nu-ansi-term) with interactive mode, headless one-shot mode, multi-session management, slash commands, path completion, and markdown rendering.
- **`brioche-provider-openai`** — Production-grade OpenAI-compatible streaming LLM client with SSE parsing, tool-call accumulation, history mirroring, chunk segmentation, and network error handling.
- **`brioche-tools-system`** — System tool executor with 5 tools (`read_file`, `write_file`, `list_dir`, `execute_command`, `fetch_url`), sandbox policies (allow-list, interactive confirm, permissive), and JSON schema generation.

### Theme 4: Cycle Rollback Telemetry
`CycleRollbackPolicy::commit_hook()` signature changed from `(&mut self)` to `(&mut self, ext: &mut ExtensionStorage)`. All governance implementations now emit structured `RollbackEvent` records into `RollbackEventLog`. `RollbackTelemetryEmitter` was **rewritten from a passive placeholder into an active consumer** that aggregates abandoned/restored counts and per-hook stats.

### Theme 5: Session Persistence Round-Trip
`SessionHeadDTO` gains `to_session(history)` — the inverse of `from_session()` — enabling full save/load cycles. Combined with `From<FlattenedAgentState> for AgentState`, persisted sessions can be reconstructed into live `Session` objects.

### Additional Major Refactors
- **`SubRoutineOrchestrator`** — Extracted 4 pure helper functions (`delegate_user_message`, `accumulate_stream_tools`, `resolve_tool_results`, `detect_subroutine_termination`) from the monolithic `handle_subroutine` method.
- **`RecoveryPolicy`** — **Complete rewrite** from a placeholder signal-consumer into a functional circuit-breaker that monitors `SessionSnapshot` for consecutive `Failure` states.
- **`SubRoutineTimeoutPolicy`** — **Complete rewrite** from a placeholder into actual timer checking with `SystemTime` epoch comparison.
- **`TransitionConflictLogger`** — **Complete rewrite** from passive observer into active aggregator stealing trace log entries and computing `TransitionConflictState`.
- **`DepthGuard`** — Extracted `calculate_depth()` as a pure function per I-Comp-Pure-Logic.
- **`ToolResultFormatter`** — Now uses `TruncatedToolResult` typed domain object instead of hand-rolled `format!()` JSON.
- **`BriocheShell`** — Added `TaskTracker` for background task health monitoring, `SessionCallback` for per-transition persistence snapshots.

---

## 2. New Crates (Complete File-by-File)

### 2.1 `crates/brioche-cli/` (10 files)

A complete terminal CLI application. **NOT listed in workspace members** — compiles standalone.

#### `Cargo.toml`
- Deps: `brioche-core`, `brioche-plugin-kit`, `brioche-shell-runtime`, `brioche-shell-persistence`, `brioche-provider-openai`, `brioche-tools-system` (all workspace)
- Deps: `reedline = "0.48"` (with `external_printer` feature), `nu-ansi-term = "0.50"`, `clap = "4.6"` (derive + env), `atty = "0.2"`, `tokio`, `tokio-util`, `async-trait`
- Bin target: `brioche-cli` → `src/main.rs`

#### `src/main.rs` (113 lines)
- Entry point with `clap` argument parser.
- Args: `--api-key` / `BRIOCHE_API_KEY`, `--model` / `BRIOCHE_MODEL` (default `gpt-4o-mini`), `--base-url` / `BRIOCHE_BASE_URL` (default `https://api.openai.com/v1`), `--one-shot` (headless mode), `--no-confirm` (disable interactive shell confirmation).
- Exits code 1 if no API key configured.
- `init_persistence()` creates `~/.local/share/brioche/sessions.redb` (fallback to `/tmp/brioche-fallback.redb`).
- Dispatches to `headless::run()` or `interactive::run()`.

#### `src/config.rs` (64 lines)
- `CliConfig { openai: OpenAiConfig, tick_interval_ms: u64 }`
- `UserConfig { api_key, model, base_url }`
- `CliConfig::from_env_and_args()` merges CLI args → env vars → defaults.

#### `src/shell_builder.rs` (172 lines)
- **`HistorySyncDecorator<T: ToolExecutor>`** — Decorator pattern synchronizing tool results with the LLM client history mirror. Separates "tool execution" from "LLM history sync" (I-Comp-Atomic-Concern).
- **`build_shell()`** — The central factory function wiring all components:
  1. Creates `ExecuteCommandTool` with optional interactive confirm handler.
  2. Builds `SystemToolExecutor` with 5 tools (read_file, write_file, list_dir, execute_command, fetch_url).
  3. Creates `OpenAiLlmClient` with broadcast channel.
  4. Spawns background task to `set_tools_schema()`.
  5. Wraps tool executor with `HistorySyncDecorator`.
  6. Creates `DefaultEffectExecutor`.
  7. Sets up `SessionCallback` that snapshots session to `SessionStore` after every transition.
  8. Builds `BriocheShell` via `PluginBuilder::standard().build_with_session()`.
  9. Supports `initial_head` / `initial_history` for session restoration.

#### `src/session_manager.rs` (71 lines)
- `SessionManager` — `BTreeMap<String, BriocheShell>` managing multiple sessions.
- Methods: `new()`, `current()`, `switch()`, `insert()`, `list()`, `current_id()`, `get()`.

#### `src/interactive.rs` (105 lines)
- Spawns **3 concurrent tasks**:
  1. **Bridge** (`bridge::run`) — async task routing REPL input to shell.
  2. **UI** (`ui::run`) — async task rendering LLM chunks via `ExternalPrinter`.
  3. **REPL** (`repl::run`) — blocking task on `tokio::task::spawn_blocking` reading lines via reedline.
- Uses `CancellationToken` for coordinated shutdown.
- Prints colored banner on startup.

#### `src/headless.rs` (91 lines)
- One-shot mode: sends a single prompt, accumulates LLM response for 30s max, prints to stdout, exits.
- Handles `LlmChunk::Text`, `ToolCallStart`/`Done`, `ToolResult`, `Done`, `Error`.
- Exit code 0 on success, 1 on error.

#### `src/bridge.rs` (285 lines)
- Async loop receiving `mpsc::Receiver<String>` from REPL.
- **Slash command handling:**
  - `/quit`, `/q` — shuts down all shells, cancels token.
  - `/help`, `/h` — prints help text.
  - `/session` — shows current session ID.
  - `/session new` — creates new session with timestamp ID, switches to it.
  - `/session list` — lists all sessions with `→` marker for current.
  - `/session load <id>` — loads persisted session from Redb storage, reconstructs via `SessionHeadDTO::to_session()`.
- Normal messages forwarded to current shell as `EngineInput::UserMessage`, with `ChatMessage::User` pushed to LLM history mirror.

#### `src/repl.rs` (170 lines)
- **`BriocheCompleter`** — Custom reedline completer:
  - Slash command completion (`/help`, `/quit`, `/session new`, etc.).
  - Path completion for words starting with `/`, `.`, or `~`.
- `run()` — Blocking loop reading lines, handling `/quit`/`Ctrl+C`/`Ctrl+D` immediately, sending other lines to bridge via `mpsc`.
- Uses `FileBackedHistory` at `/tmp/brioche-cli-history.txt`.

#### `src/ui.rs` (142 lines)
- Async loop receiving `broadcast::Receiver<LlmChunk>`.
- **Renders markdown** in terminal: inline code (`` ` ``), bold (`**`), italic (`*`), headers (`# `, `## `, `### `), bullets (`- `).
- Displays tool calls and results with colored icons (⚙, ✓, ✗).
- Prints response **as a single block** when stream completes (avoids reedline re-rendering artifacts).

---

### 2.2 `crates/brioche-provider-openai/` (5 files)

Production OpenAI-compatible LLM provider. **Listed in workspace members.**

#### `Cargo.toml`
- Deps: `brioche-core`, `brioche-shell-runtime` (workspace), `reqwest`, `async-trait`, `bytes`, `futures-util`, `serde`, `serde_json`, `thiserror`, `tokio`, `tracing`

#### `src/lib.rs` (32 lines)
- Exports: `OpenAiLlmClient`, `OpenAiConfig`, `LlmChunk`, `SharedHistory`.

#### `src/config.rs` (36 lines)
- `OpenAiConfig { api_key, model, base_url, max_tokens, timeout_ms }`
- Defaults: model=`gpt-4o-mini`, base_url=`https://api.openai.com/v1`, max_tokens=4096, timeout=120s.

#### `src/client.rs` (467 lines)
- **`LlmChunk` enum** — Broadcast to projection layer:
  - `Text(String)`, `ToolCallStart { id, name }`, `ToolArgument { id, fragment }`, `ToolCallDone { id }`, `ToolResult { name, output }`, `Done`, `Error(String)`
- **`OpenAiLlmClient`**:
  - `new(config)` → `(client, broadcast_rx, shared_history)`
  - `subscribe()` — gets new broadcast receiver
  - `push_message()` — adds to history mirror
  - `set_tools_schema()` — updates tool schemas dynamically
  - `call_llm()` — full `LlmClient` trait implementation:
    1. Builds request body from history mirror + tool schemas.
    2. Sends POST to `/chat/completions` with streaming.
    3. On network error → emits `SystemSignal::NetworkUnavailable` + `LlmChunk::Error`.
    4. Parses SSE stream via `SseParser`.
    5. For text deltas → `emit_text_chunk()` (segments per `MAX_INLINE_CHUNK`, sends `EngineInput::LlmStream(TextChunk)` + broadcasts `LlmChunk::Text`).
    6. For tool call deltas → accumulates in `BTreeMap<usize, ToolCallAccumulator>`, emits `ToolCallStart`/`ToolArgument`/`ToolCallDone`.
    7. On `finish_reason="tool_calls"` → emits `ToolCallDone`.
    8. On stream end → persists `pending_text` to history mirror as `ChatMessage::Assistant`, sends `StreamEvent::Done`.
  - `emit_tool_result()` / `push_tool_results()` — called by `HistorySyncDecorator` to sync tool results back to LLM history.

#### `src/request.rs` (84 lines)
- `build_messages(history)` — converts `ChatMessage` variants (`System`, `User`, `Assistant`, `ToolRequest`, `ToolResult`) to OpenAI JSON format.
- `build_request_body(model, messages, max_tokens, tools)` — assembles JSON payload with optional `tools` + `tool_choice: "auto"`.

#### `src/sse.rs` (84 lines)
- `SseParser` — Line-buffered SSE parser.
- `feed(&Bytes)` → `Iterator<serde_json::Value>`.
- Handles fragmented chunks across calls.
- Ignores `data: [DONE]`.
- Unit tests: single line, ignores done, splits fragmented.

---

### 2.3 `crates/brioche-tools-system/` (6 files)

System tool bindings. **Listed in workspace members.**

#### `Cargo.toml`
- Deps: `brioche-core`, `brioche-shell-runtime` (workspace), `serde_json`, `tokio`, `thiserror`

#### `src/lib.rs` (21 lines)
- Crate docs in French. Exports: `SystemTool`, `SystemToolExecutor`, `ToolError`, `AllowList`, `SandboxPolicy`, plus 5 tool structs.

#### `src/registry.rs` (155 lines)
- **`SystemTool` trait** — `name()`, `description()`, `parameters_schema()`, `run(args, cancel) -> Result<String, ToolError>`.
- **`SystemToolExecutor`** — `BTreeMap<String, Box<dyn SystemTool>>` implementing `ToolExecutor`:
  - `with_tool()` — builder pattern registering tools.
  - `schema_json()` — generates OpenAI `tools` array format.
  - `run_tool()` — delegates to registered tool, maps `ToolError` variants to `ToolOutcome`.
- **`ToolError`** enum: `SandboxDenied`, `Io`, `InvalidArgs`, `NotFound`.

#### `src/sandbox.rs` (78 lines)
- **`SandboxPolicy`** enum: `Permissive`, `AllowList(AllowList)`, `Interactive`.
- **`AllowList`** — `BTreeSet<String>` of allowed commands.
- Default allow-list: `ls`, `cat`, `grep`, `find`, `git`, `cargo`, `rustc`, `pwd`, `echo`, `head`, `tail`, `wc`.
- `ConfirmHandler` type — `Arc<dyn Fn(&str) -> bool + Send + Sync>`.

#### `src/tools/filesystem.rs` (135 lines)
- `ReadFileTool` — reads text file via `tokio::fs::read_to_string`.
- `WriteFileTool` — writes text file via `tokio::fs::write`.
- `ListDirTool` — lists directory entries with `dir`/`file` prefix.
- All use JSON schema parameters (`path`, `content`).

#### `src/tools/shell.rs` (197 lines)
- `ExecuteCommandTool` — runs `sh -c "command"` via `tokio::process::Command`.
- Supports `cwd` parameter.
- **Cancellation support** — uses `tokio::select!` with `cancel.cancelled()`.
- **Sandbox enforcement**:
  - `Permissive` — allows all, logs warning.
  - `AllowList` — blocks unless command in list; if confirm handler configured, prompts user in `spawn_blocking`.
  - `Interactive` — always prompts for confirmation.
- Captures stdout + stderr; returns error on non-zero exit code.

#### `src/tools/web.rs` (76 lines)
- `FetchUrlTool` — HTTP GET via `reqwest`.
- Supports cancellation via `tokio::select!`.
- Returns error on non-2xx status.

---

## 3. Core Changes (`brioche-core`)

### 3.1 `src/types.rs` (+151 lines)

**Added `TruncatedToolResult`** (lines 206–226):
```rust
pub struct TruncatedToolResult {
    pub truncated: bool,
    pub original_len: usize,
    pub preview: String,
}
```
- `from_content(content, max_bytes)` — creates truncation record.
- `to_json()` — serializes to JSON string (infallible).
- Replaces hand-rolled `format!("{{\"truncated\":true,...}}")` in `ToolResultFormatter`.

**Added `RollbackEventLog` and `RollbackEvent`** (lines 228–265):
```rust
#[derive(BriocheExtensionType)]
pub struct RollbackEventLog {
    #[brioche(deterministic_order)]
    pub events: Vec<RollbackEvent>,
}

pub struct RollbackEvent {
    pub hook_name: String,
    pub was_rollback: bool,
    pub frame_weight: usize,
    pub budget_exceeded: bool,
}
```
- Written by `CycleRollbackPolicy` implementations during `commit_hook`/`rollback_hook`.
- Consumed by `RollbackTelemetryEmitter`.

**Added `Session.pending_assistant_text`** (lines 293–301):
```rust
pub pending_assistant_text: String,
```
- Buffer for LLM streaming text fragments.
- Mechanical field — never exposed to plugins.
- Added to `Debug` output as `pending_assistant_len`.
- Initialized to `String::new()` in `Session::new()`.

**Added `UiWidget` enum** (lines 614–691):
```rust
pub enum UiWidget {
    TextChunk { trace_id: String, text: String },
    Error { code: String, message: String },
    CriticalError { component: String, detail: Option<String> },
    SystemDegraded { plugin: String },
    NetworkError { reason: String },
    Status(String),
    SubRoutineTimeout { handle: SubRoutineHandle, limit_ms: u64 },
    SubRoutineLoaded { handle: SubRoutineHandle },
    PendingTask { task_id: String, status: String },
    Test { msg: String },
    Custom { widget_type: String, payload: serde_json::Value },
}
```
- `widget_type()` method returns canonical string for backward compatibility with frontend registry.

**Changed `Effect::ForwardToUi`** (line 703):
```rust
// BEFORE:
ForwardToUi { widget_type: String, payload: serde_json::Value }
// AFTER:
ForwardToUi(UiWidget)
```

**Updated `effect_to_bitmask`** (line 904): `Effect::ForwardToUi(_)` pattern.

### 3.2 `src/engine.rs` (+24 / −7 lines)

**`commit_hook` signature update** (line 422):
```rust
// BEFORE: r.commit_hook();
// AFTER:  r.commit_hook(&mut session.extensions);
```

**Assistant text accumulation** (lines 630–635):
```rust
StreamEvent::TextChunk { chunk, .. } => {
    session.pending_assistant_text.push_str(&String::from_utf8_lossy(chunk));
}
```

**Persist assistant text on `ToolCallDone`** (lines 663–668):
```rust
if !session.pending_assistant_text.is_empty() {
    session.history.push(ChatMessage::Assistant {
        content: std::mem::take(&mut session.pending_assistant_text),
    });
}
```

**Persist assistant text on `Done`** (lines 700–705): Same pattern.

### 3.3 `src/plugin.rs` (+2 / −1 lines)

**`CycleRollbackPolicy::commit_hook` signature** (line 280):
```rust
// BEFORE: fn commit_hook(&mut self);
// AFTER:  fn commit_hook(&mut self, ext: &mut ExtensionStorage);
```

### 3.4 `src/extension.rs` (+15 / −6 lines)

**Comment rewrite only** — no behavior change. Rewrote `Box::leak` fallback comment for clarity, removing `SAFETY:` header and `debug_assert!(false)` explanation.

### 3.5 `src/lib.rs` (+9 / −6 lines)

**Added re-exports:**
```rust
RollbackEvent, RollbackEventLog, TruncatedToolResult, UiWidget
```

### 3.6 `tests/engine_transition.rs` (+124 lines)

**Added 2 new tests:**
1. `transition_llm_stream_accumulates_assistant_text` — Tests that text chunks accumulate into `pending_assistant_text` and materialize as `ChatMessage::Assistant` at `Done`.
2. `transition_llm_stream_tool_call_done_persists_preceding_text` — Tests that assistant text preceding tool calls is persisted before state transitions to `ExecutingTools`.

**Updated existing tests:**
- `OverrideInputPlugin` now returns `Effect::ForwardToUi(UiWidget::Test { msg: "overridden".into() })` instead of JSON payload.
- `transition_override_input_short_circuits` assertion updated to `widget.widget_type() == "test"`.
- `transition_with_system_failover_guard_replaces_fault` assertion updated to `widget.widget_type() == "critical_error"`.

---

## 4. Shell Runtime Changes (`brioche-shell-runtime`)

### 4.1 `src/shell.rs` (+118 / −16 lines)

**Added `SessionCallback` type** (lines 114–120):
```rust
pub type SessionCallback = Box<dyn Fn(&Session) + Send>;
```
- Called on engine thread after each transition.
- Used by CLI to snapshot session for persistence.

**Added `TaskTracker`** (lines 123–170):
```rust
pub struct TaskTracker {
    handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
}
```
- `spawn(future)` — launches task and stores handle.
- `health_check()` — verifies all tasks are still active; logs `tracing::error` for finished tasks.
- Replaces the previous pattern of fire-and-forget `tokio::spawn`.

**`BriocheShell::new()` signature changed** (lines 277–282):
```rust
pub fn new<F, E>(
    engine_factory: F,
    config: ShellConfig,
    executor: E,
    session_callback: Option<SessionCallback>,  // NEW
) -> Self
```

**`BriocheShell` fields changed** (lines 243–247):
```rust
// REMOVED: config: Arc<ShellConfig>,
// ADDED:   task_tracker: TaskTracker,
```

**Background tasks now tracked** (lines 347, 359, 363):
```rust
task_tracker.spawn(async move { ... });
```
- Effect consumption loop, tick emitter, engine watchdog all tracked.

**Added `health_check()` method** (lines 465–470):
```rust
pub fn health_check(&self) -> bool {
    self.task_tracker.health_check()
}
```

**`engine_thread_loop` invokes callback** (lines 553–558):
```rust
if let Some(ref cb) = session_callback {
    cb(&session);
}
```

**Effect dispatch updated** (lines 649–651):
```rust
Effect::ForwardToUi(widget) => {
    executor.forward_to_ui(widget).await?;
}
```

### 4.2 `src/effect_executor.rs` (+16 / −11 lines)

**`forward_to_ui` signature changed** (lines 70–73, 286–289):
```rust
// BEFORE: async fn forward_to_ui(&self, widget_type: String, payload: serde_json::Value) -> Result<...>;
// AFTER:  async fn forward_to_ui(&self, widget: UiWidget) -> Result<...>;
```
- Added `UiWidget` to imports.

### 4.3 `src/lib.rs` (+2 / −1 lines)

**Added `SessionCallback` to re-exports.**

### 4.4 `tests/shell_runtime.rs` (+5 lines)

All `BriocheShell::new()` calls updated to pass `None` as 4th argument for `session_callback`.

---

## 5. Shell Projection Changes (`brioche-shell-projection`)

### 5.1 `src/content_renderer.rs` (+31 / −31 lines)

**Complete rewrite of `process_effect()`:**
```rust
// BEFORE: Destructured ForwardToUi { widget_type, payload }, checked widget_type == "text_chunk",
//         probed JSON for "trace_id" and "text" fields.
// AFTER:  Destructures ForwardToUi(widget), matches UiWidget::TextChunk { trace_id, text }.
```
- Uses direct enum destructuring instead of JSON probing.

### 5.2 `src/ui_composer.rs` (+68 / −40 lines)

**`classify_widget` completely rewritten:**
```rust
// BEFORE: fn classify_widget(widget_type: &str) -> EffectPriority
// AFTER:  fn classify_widget(widget: &UiWidget) -> EffectPriority
```
- Now matches on `UiWidget` enum variants exhaustively.
- `UiWidget::TextChunk { .. }` → `TextChunk`
- `UiWidget::Custom { widget_type, .. }` with `"focus"` / `"scroll"` → `Navigation`
- `UiWidget::Error` / `CriticalError` / `SystemDegraded` / `NetworkError` / `Status` / `SubRoutineTimeout` / `SubRoutineLoaded` / `PendingTask` → `Semantic`
- `UiWidget::Custom` with `"accordion_expand"` / `"highlight"` / `"subroutine_loaded"` or `is_special_widget()` → `Semantic`
- `UiWidget::Custom` with `"animation"` / `"transition"` / `"spinner"` → `Cosmetic`
- `UiWidget::Test { .. }` → `Cosmetic`
- All other `UiWidget::Custom` → `Cosmetic`

All call sites updated from `Effect::ForwardToUi { widget_type, .. }` to `Effect::ForwardToUi(widget)`.

### 5.3 `src/ui_performance_policy.rs` (+2 / −2 lines)

Pattern updated: `matches!(effect, Effect::ForwardToUi(_))`.

### 5.4 `src/ui_registry.rs` (+8 / −5 lines)

Docs updated to reflect structured `UiWidget` enum and `UiWidget::Custom` fallback.

### 5.5 `src/widget.rs` (+8 / −5 lines)

Docs updated — constants now described as canonical string identifiers mapped via `UiWidget::widget_type()`.

### 5.6 `src/ipc_command.rs` (+1 line)

Test helper `build_shell()` passes `None` for new `session_callback` parameter.

### 5.7 `tests/projection_tests.rs` (+107 / −107 lines)

**Every single test rewritten** to use `UiWidget` variants:
- `make_text_chunk_effect()` now uses `UiWidget::TextChunk { trace_id, text }`.
- `content_renderer_ignores_non_text_chunk()` uses `UiWidget::Status("ok".into())`.
- `ui_composer_enqueues_forward_to_ui()` uses `UiWidget::TextChunk`.
- `ui_composer_text_chunk_never_dropped()` uses `UiWidget::TextChunk`.
- `ui_composer_cosmetic_dropped_after_3_frames()` uses `UiWidget::Custom`.
- `ui_composer_priority_ordering()` uses `UiWidget::Custom` and `UiWidget::TextChunk`.
- `ui_performance_policy_process_effects_separates_ui_and_non_ui()` uses `UiWidget::TextChunk`.
- `special_widget_maps_to_semantic_priority()` uses `UiWidget::SystemDegraded`.
- All assertions use `widget.widget_type()` instead of direct string comparison.

---

## 6. Shell Persistence Changes (`brioche-shell-persistence`)

### 6.1 `src/dto.rs` (+32 lines)

**Added `From<FlattenedAgentState> for AgentState`** (lines 63–78):
- Inverse of existing `From<&AgentState> for FlattenedAgentState`.
- Maps all variants including `SubRoutine(handle)` via `SubRoutineHandle::new()`.

**Added `SessionHeadDTO::to_session()`** (lines 136–151):
```rust
pub fn to_session(&self, history: Vec<ChatMessage>) -> Session {
    let mut session = Session::new(&self.id);
    session.history = history;
    session.persisted_msg_count = self.persisted_msg_count;
    session.state = self.state.clone().into();
    session.state_stack = self.state_stack.iter().cloned().map(Into::into).collect();
    session
}
```
- Enables full round-trip: `Session` → `SessionHeadDTO` + messages → `Session`.

### 6.2 `Cargo.toml` (+2 / −2 lines)

`lru` bumped `0.16` → `0.18`.

---

## 7. Governance Changes (`brioche-governance-default`)

### 7.1 `src/adaptive_undo_frame_guard.rs` (+40 / −14 lines)

**Structural changes:**
- Added `budget_policy: Option<Box<dyn CowBudgetPolicy>>` field (was previously `#[allow(dead_code)]` on `current_hook`).
- Added `with_budget_policy(policy)` builder method.
- `effective_max()` now actually consults the policy:
  ```rust
  match &self.budget_policy {
      Some(policy) => policy.max_cow_bytes(&self.current_hook),
      None => self.fallback_max_cow_bytes,
  }
  ```

**Telemetry emission:**
- `commit_hook(ext)` — appends `RollbackEvent { was_rollback: false, frame_weight, budget_exceeded }` to `RollbackEventLog`.
- `rollback_hook(ext)` — appends `RollbackEvent { was_rollback: true, frame_weight, budget_exceeded }` after restoration.

### 7.2 `src/depth_guard.rs` (+49 / −31 lines)

**Extracted pure function** `calculate_depth(stack_depth, current_state)` (lines 53–62):
```rust
pub fn calculate_depth(stack_depth: usize, current_state: AgentStateTag) -> u64 {
    if current_state == AgentStateTag::SubRoutine {
        stack_depth as u64 + 1
    } else {
        stack_depth as u64
    }
}
```

**Simplified `on_input`** — replaced inline depth calculation with pure function call.

**Typed `ForwardToUi`** — now returns:
```rust
Effect::ForwardToUi(UiWidget::Error {
    code: "DEPTH_LIMIT_EXCEEDED".into(),
    message: format!("sub-routine depth limit exceeded: {} >= {}", current_depth, self.max_depth),
})
```
- Replaced hand-rolled JSON payload construction.

### 7.3 `src/recovery_policy.rs` (+83 / −44 lines)

**COMPLETE REWRITE** from placeholder to functional circuit breaker.

**State changes:**
```rust
// BEFORE:
pub struct RecoveryState {
    pub consecutive_recoveries: u64,
    pub last_network_error: Option<String>,
}

// AFTER:
pub struct RecoveryState {
    pub consecutive_recoveries: u64,
    pub last_error: Option<String>,
    pub inputs_blocked: u64,
}
```

**Policy struct changes:**
```rust
// BEFORE: pub struct RecoveryPolicy;
// AFTER:  pub struct RecoveryPolicy { max_consecutive_recoveries: u64 }
```
- `new()` — default threshold 3.
- `with_max_recoveries(n)` — custom threshold.

**Algorithm rewrite** — `on_input` now:
1. Reads `SessionSnapshot`, checks `current_state == Failure`.
2. If failure → increments `consecutive_recoveries`, stores `last_error`.
3. If `consecutive_recoveries >= max_consecutive_recoveries` → increments `inputs_blocked`, returns `PolicyDecision::Block`.
4. If healthy and circuit not open → resets counter.
5. Returns `PolicyDecision::Allow`.

### 7.4 `src/rollback_telemetry_emitter.rs` (+79 / −26 lines)

**COMPLETE REWRITE** from passive placeholder to active consumer.

**State changes:**
```rust
// BEFORE: BTreeMap<String, (u64, u64)> per_hook_stats
// AFTER:  Vec<(String, u64, u64)> per_hook_stats with #[brioche(deterministic_order)]
```
- Added `abandoned_count`, `restored_count`, `abandoned_weight_total`.

**Algorithm rewrite** — `after_prediction` now:
1. Steals events from `RollbackEventLog` via `std::mem::take`.
2. For each event:
   - `was_rollback && budget_exceeded` → `abandoned_count++`, `abandoned_weight_total += frame_weight`
   - `was_rollback && !budget_exceeded` → `restored_count++`
   - Updates `per_hook_stats` via linear scan.

### 7.5 `src/subroutine_orchestrator.rs` (+290 / −131 lines)

**MAJOR REFACTOR** — extracted 4 pure helper functions from monolithic `handle_subroutine`.

**Added `delegate_user_message(child, content)`** (lines 36–53):
- Pushes `ChatMessage::User`, transitions child to `Predicting`, returns `[CallLlmNetwork, SaveSession]`.

**Added `accumulate_stream_tools(child, event)`** (lines 56–112):
- Handles `ToolCallStart` → inserts into `StreamToolAccumulator`.
- Handles `ToolArgumentChunk` → appends to descriptor arguments.
- Handles `ToolCallDone` → extracts pending tools, seals them, sets `child.active_tools`, transitions to `ExecutingTools`, returns `[ExecuteTools, SaveSession]`.

**Added `resolve_tool_results(child, generation_id, results)`** (lines 115–147):
- Pops state, clears `active_tools`.
- Converts `ToolResultDTO` outcomes to `ChatMessage::ToolResult` entries.
- Transitions back to `Predicting`.

**Added `detect_subroutine_termination(parent, child)`** (lines 150–176):
- `Idle` → extracts last child message, pushes to parent, pops parent state, returns `[SaveSession, CallLlmNetwork]`.
- `Failure` → pushes `"sub-routine failed"` system message, pops parent state, returns `[SaveSession, CallLlmNetwork]`.
- Other → returns `None`.

**Trait implementation** now thin orchestration:
```rust
EngineInput::UserMessage(content) => delegate_user_message(child, content).map(Some).map_err(wrap),
EngineInput::LlmStream(event) => accumulate_stream_tools(child, event).map_err(wrap),
EngineInput::ToolCallsResult { generation_id, results } => {
    resolve_tool_results(child, *generation_id, results).map_err(wrap)?;
    detect_subroutine_termination(parent, child).map_err(wrap)
}
```

### 7.6 `src/subroutine_timeout_policy.rs` (+71 / −27 lines)

**COMPLETE REWRITE** from placeholder to functional timer checker.

**Struct simplified:**
```rust
// BEFORE: SubRoutineTimeoutPolicy { default_timeout_ms: u64 }
// AFTER:  SubRoutineTimeoutPolicy;
```
- `with_default_timeout()` now ignores parameter and returns `new()` (API compatibility).

**Algorithm rewrite** — `on_input` now:
1. Checks `SessionSnapshot.current_state == SubRoutine`.
2. If not in sub-routine → clears stale timers, returns `Allow`.
3. Gets current epoch time.
4. Scans `SubRoutineTimerState.timers` for expired entries (`now - start > limit`).
5. If expired → removes timer, returns `Block { reason }`.
6. Returns `Allow`.

### 7.7 `src/system_failover_guard.rs` (+18 / −18 lines)

**Typed `ForwardToUi`** — replaced JSON payload with `UiWidget::CriticalError`:
```rust
Effect::ForwardToUi(UiWidget::CriticalError {
    component: plugin_name,
    detail: Some("governance component failed; system degraded".into()),
})
```

### 7.8 `src/tiered_undo_frame_guard.rs` (+17 / −2 lines)

**Telemetry emission** — same pattern as `AdaptiveUndoFrameGuard`:
- `commit_hook(ext)` — logs `RollbackEvent { was_rollback: false, frame_weight, budget_exceeded }`.
- `rollback_hook(ext)` — logs `RollbackEvent { was_rollback: true, frame_weight, budget_exceeded }`.

### 7.9 `src/tool_result_formatter.rs` (+9 / −6 lines)

**Uses `TruncatedToolResult`** (line 102–103):
```rust
let meta = TruncatedToolResult::from_content(&content, self.max_result_bytes);
result.outcome = ToolOutcome::Success(meta.to_json());
```
- Replaced hand-rolled `format!("{{\"truncated\":true,...}}")`.

### 7.10 `src/transition_conflict_logger.rs` (+56 / −20 lines)

**COMPLETE REWRITE** from passive observer to active aggregator.

**Added `TransitionConflictState`** (lines 21–38):
```rust
pub struct TransitionConflictState {
    pub total_conflicts: u64,
    pub unique_preempted_plugins: u64,
    pub last_preempted_plugin: Option<String>,
}
```

**Algorithm rewrite** — `after_prediction` now:
1. Steals entries from `SupersededTransitionTraceLog` via `std::mem::take`.
2. Updates `total_conflicts` by entry count.
3. Counts unique `preempted_by` plugins.
4. Stores `last_preempted_plugin`.

### 7.11 `src/undo_frame_guard.rs` (+17 / −2 lines)

**Telemetry emission** — same pattern:
- `commit_hook(ext)` — logs `RollbackEvent { was_rollback: false, ... }`.
- `rollback_hook(ext)` — logs `RollbackEvent { was_rollback: true, ... }`.

### 7.12 `src/noop_traits.rs` (+2 / −2 lines)

**Signature update:** `fn commit_hook(&mut self, _ext: &mut ExtensionStorage) {}`

### 7.13 `src/lib.rs` (+2 / −1 lines)

**Added `TransitionConflictState` to re-exports.**

---

## 8. Standard Library Changes (`brioche-std`)

### 8.1 `src/tool_result_policy.rs` — DELETED (−107 lines)

Entire file removed. Functionality superseded by `ToolResultFormatter` in `brioche-governance-default`.

### 8.2 `src/lib.rs` (+2 / −3 lines)

- Removed `pub mod tool_result_policy;`.
- Removed `pub use tool_result_policy::*;`.

### 8.3 `tests/standard_plugins.rs` (+29 / −29 lines)

**Rewritten to use `ToolResultFormatter`:**
- `tool_result_policy_truncates_oversized` → `tool_result_formatter_truncates_oversized`
- `tool_result_policy_passes_small_results` → `tool_result_formatter_passes_small_results`
- Uses `ToolResultFormatterState` instead of `ToolResultPolicyState`.
- Checks `formatted_count` instead of `results_truncated` / `results_processed`.
- `engine_with_all_std_plugins_runs_user_message` uses `ToolResultFormatter::default()` instead of `ToolResultPolicy::default()`.

---

## 9. Plugin Kit & Macro Changes

### 9.1 `brioche-plugin-kit/src/lib.rs` (+3 / −2 lines)

**Removed re-exports:**
```rust
// REMOVED:
pub use brioche_std::{ToolResultPolicy, ToolResultPolicyState};
```

### 9.2 `brioche-macro/tests/ui/fail_manual_impl.stderr` (+1 line)

Compiler error help text updated — `RollbackEventLog` added to the list of types implementing the sealed trait:
```
= help: the following other types implement trait ...:
            EpochState
            RollbackEventLog    // NEW
            SessionSnapshot
            SignalBuffer
            StreamToolAccumulator
```

---

## 10. Workspace & Dependency Changes

### 10.1 `Cargo.toml` (root) (+7 lines)

**Added workspace dependencies:**
```toml
async-trait = "0.1"
bytes = "1"
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["rt"] }
```

**Added workspace members:**
```toml
brioche-provider-openai = { path = "crates/brioche-provider-openai" }
brioche-tools-system = { path = "crates/brioche-tools-system" }
```

**Note:** `brioche-cli` is **NOT** in workspace members.

### 10.2 `Cargo.lock` (+1,638 lines)

Massive update due to new crates bringing in:
- `reqwest`, `hyper`, `h2`, `http`, `tokio-util`, `futures-util`, `async-trait`, `clap`, `reedline`, `nu-ansi-term`, `atty`, `windows-sys 0.61.2`, `android_system_properties`, `atomic-waker`
- `thiserror` versions unified (removed pinned `1.0.69` / `2.0.18` suffixes).

### 10.3 `brioche-shell-persistence/Cargo.toml`

`lru` `0.16` → `0.18`.

### 10.4 `brioche-shell-projection/Cargo.toml`

`indexmap` `2.7` → `2.14`.

### 10.5 `brioche-shell-runtime/Cargo.toml`

`thiserror` `1` → `2`.

---

## 11. Documentation Changes

### 11.1 `SPECS.md` (+265 / −113 lines)

**Major updates:**

1. **Added CUPID/SCIFI/FP/DOD references** — New paragraph in §1 introduction, new §1.4 "Compositional design principles" with code examples, new glossary entries, new invariants 45–48.

2. **`ForwardToUi` rewritten everywhere** — All occurrences of `ForwardToUi { widget_type, payload }` changed to `ForwardToUi(UiWidget::...)`.

3. **`CycleRollbackPolicy::commit_hook(ext)`** — Updated all references to include `ext` parameter.

4. **Added `UiWidget` enum definition** — New subsection §2.4 with full enum definition.

5. **Updated `SubRoutineHandler` algorithm** — Describes pure helper functions (`delegate_user_message`, `accumulate_stream_tools`, `resolve_tool_results`, `detect_subroutine_termination`).

6. **Updated `RecoveryPolicy` algorithm** — Complete rewrite describing circuit breaker behavior.

7. **Updated `TransitionConflictLogger` algorithm** — Describes active aggregation into `TransitionConflictState`.

8. **Updated `RollbackTelemetryEmitter` algorithm** — Describes consumption of `RollbackEventLog`.

9. **Updated `AdaptiveUndoFrameGuard` algorithms** — Describes telemetry event appending.

10. **Updated `ToolResultPolicy` → `ToolResultFormatter`** — Renamed section, updated algorithm to use `TruncatedToolResult`.

11. **Added composition invariants:**
    - `I-Comp-Atomic-Concern`
    - `I-Comp-Pure-Logic`
    - `I-Comp-Typed-Effects`
    - `I-Comp-Trait-Capability`

12. **Updated invariant table** — Added invariants 45–48.

13. **Updated `SubRoutineCache`** — `l1_visible` changed from `HashMap` to `BTreeMap` (determinism).

14. **Updated `ExtensionStorage` docs** — Added DOD cache locality note.

15. **Updated UI projection docs** — Reflects structured `UiWidget` enum.

### 11.2 `docs/PHILOSOPHY.md` (+113 lines)

**New §7: "Compositional Design Canon: CUPID, SCIFI, Functional Programming, and Data-Oriented Design"**

- **7.1 CUPID** — Table mapping each property to Brioche manifestation.
- **7.2 SCIFI** — Five principles with Brioche examples.
- **7.3 Functional Programming** — Data/behavior separation, effects as values, no hidden I/O.
- **7.4 Data-Oriented Design** — Entities as identifiers, behavior-free data, cache-conscious access, pre-computed dispatch.
- **7.5 Design Rules** — Five operational rules with code examples:
  1. One Concern Per Plugin (Unix Philosophy)
  2. Extract Pure Functions from Hooks (FP)
  3. No Stringly-Typed Holes in Effect (Domain-Based)
  4. Traits Are Capabilities, Not Taxonomies (Composable)
  5. Document the Data Layout (DOD)

**Renumbered summary** — Old §7 became §8. Added 5 new rows to the non-negotiables table.

---

## 12. PR Checklist & Breaking Changes

### 12.1 Breaking Changes

| Change | Migration Required |
|--------|-------------------|
| `Effect::ForwardToUi { widget_type, payload }` → `Effect::ForwardToUi(UiWidget)` | All code constructing `ForwardToUi` must use `UiWidget` variants |
| `CycleRollbackPolicy::commit_hook(&mut self)` → `commit_hook(&mut self, ext: &mut ExtensionStorage)` | All custom governance plugins implementing this trait must update signature |
| `ToolResultPolicy` removed from `brioche-std` | Use `ToolResultFormatter` from `brioche-governance-default` |
| `BriocheShell::new()` takes 4th argument `session_callback: Option<SessionCallback>` | All call sites must pass `None` or a callback |
| `EffectExecutor::forward_to_ui(widget: UiWidget)` instead of `(widget_type, payload)` | Custom effect executors must update signature |
| `lru` 0.16 → 0.18 | Recompile sufficient |
| `indexmap` 2.7 → 2.14 | Recompile sufficient |
| `thiserror` 1 → 2 (in `brioche-shell-runtime`) | Recompile sufficient |

### 12.2 Pre-Merge Checklist

- [x] `cargo check` passes on new crates
- [ ] `cargo test --workspace` — must validate all updated tests
- [ ] `cargo clippy --workspace` — new crates may have warnings
- [ ] `cargo-brioche-lint-invariants` — verify new patterns pass
- [ ] Decide whether to add `brioche-cli` to workspace members
- [ ] Verify `brioche-cli` compiles standalone (`cargo check --manifest-path crates/brioche-cli/Cargo.toml`)

### 12.3 Suggested PR Title

> **feat: typed UI widgets (`UiWidget`), LLM streaming text buffering, CLI/provider/tool crates, and governance refactors**

### 12.4 Files by Change Category

| Category | Files |
|----------|-------|
| **New crates** | `crates/brioche-cli/*` (10 files), `crates/brioche-provider-openai/*` (5 files), `crates/brioche-tools-system/*` (6 files) |
| **Core types** | `crates/brioche-core/src/types.rs`, `crates/brioche-core/src/lib.rs` |
| **Core engine** | `crates/brioche-core/src/engine.rs`, `crates/brioche-core/src/plugin.rs`, `crates/brioche-core/src/extension.rs` |
| **Core tests** | `crates/brioche-core/tests/engine_transition.rs` |
| **Shell runtime** | `crates/brioche-shell-runtime/src/shell.rs`, `src/effect_executor.rs`, `src/lib.rs`, `tests/shell_runtime.rs` |
| **Shell projection** | `crates/brioche-shell-projection/src/content_renderer.rs`, `src/ui_composer.rs`, `src/ui_performance_policy.rs`, `src/ui_registry.rs`, `src/widget.rs`, `src/ipc_command.rs`, `tests/projection_tests.rs` |
| **Shell persistence** | `crates/brioche-shell-persistence/src/dto.rs` |
| **Governance** | `crates/brioche-governance-default/src/adaptive_undo_frame_guard.rs`, `src/depth_guard.rs`, `src/recovery_policy.rs`, `src/rollback_telemetry_emitter.rs`, `src/subroutine_orchestrator.rs`, `src/subroutine_timeout_policy.rs`, `src/system_failover_guard.rs`, `src/tiered_undo_frame_guard.rs`, `src/tool_result_formatter.rs`, `src/transition_conflict_logger.rs`, `src/undo_frame_guard.rs`, `src/noop_traits.rs`, `src/lib.rs` |
| **Std** | `crates/brioche-std/src/lib.rs`, `tests/standard_plugins.rs` |
| **Plugin kit** | `crates/brioche-plugin-kit/src/lib.rs` |
| **Macro** | `crates/brioche-macro/tests/ui/fail_manual_impl.stderr` |
| **Workspace** | `Cargo.toml`, `Cargo.lock` |
| **Docs** | `SPECS.md`, `docs/PHILOSOPHY.md` |

---

*End of complete report.*
