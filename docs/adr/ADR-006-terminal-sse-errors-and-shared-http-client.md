# ADR-006: Terminal SSE Errors and Shared HTTP Client Hardening

## Status

Accepted

## Context

P0-SEC-04 and P0-SEC-06 require the shell/runtime boundary to handle provider and transport failures deterministically:

- P0-SEC-04: When the OpenAI provider fails to send a request or read the SSE stream, the kernel must receive a terminal event so the current prediction is finalised rather than left dangling.
- P0-SEC-06: Network calls made by the OpenAI provider, system tools, and user-defined HTTP tools must share safe defaults (timeouts, redirect limits, URL allow-lists, response size limits) to prevent SSRF and unbounded memory consumption.

These changes touch Book I (kernel dispatch), Book III-A (`brioche-shell-runtime`), Book III-C (`brioche-tools-system`), and the OpenAI provider crate. A cross-book ADR is therefore required.

## Decision

### 1. Add `StreamEvent::Error { message }` as a terminal stream event

The kernel already finalises predictions on `StreamEvent::Done` and `StreamEvent::ToolCallDone`. `StreamEvent::Error` is treated as a third terminal variant:

- `dispatch_llm_stream` calls `finalize_prediction_text_only` when `Error` is received.
- `accumulate_stream_event` performs no mechanical accumulation for `Error`.
- `property_transition.rs` includes `StreamEvent::Error` in the input strategy so determinism and no-panic properties are checked.

**Rationale**: A transport/provider failure is semantically equivalent to an incomplete stream. The kernel must reach a well-defined terminal state without introducing a separate failure path.

### 2. Centralise safe HTTP defaults in `brioche-shell-runtime::http_client`

A new module exposes:

- `build_http_client` for constructing a `reqwest::Client` with timeout and redirect limits.
- `validate_url` for scheme and host allow-list validation.
- `read_body_with_size_limit` for bounded response body reads.
- Default constants: `DEFAULT_REQUEST_TIMEOUT`, `DEFAULT_CONNECT_TIMEOUT`, `DEFAULT_MAX_REDIRECTS`, `DEFAULT_MAX_RESPONSE_BYTES`, `ALLOWED_SCHEMES`, `BLOCKED_HOSTS`.

**Rationale**: Consolidating network policy in the shell runtime keeps Core pure and prevents each consumer from inventing its own (possibly weaker) defaults.

### 3. Consume the shared client in OpenAI provider, `fetch_url`, and user-defined HTTP tools

- `OpenAiLlmClient::send_request` validates the request URL and emits `StreamEvent::Error` on failure.
- `OpenAiLlmClient::read_sse_stream` emits `StreamEvent::Error` on idle timeout, SSE errors, and parser aborts.
- `FetchUrlTool::run` and `execute_http_post` validate URLs and use the shared client builder/size limiter.

**Rationale**: Uniform enforcement guarantees that no shell-side HTTP caller bypasses the allow-list or size limits.

## Consequences

### Positive

- Transport failures always produce a terminal kernel event.
- SSRF surface is reduced by centralised URL validation.
- Response size limits prevent OOM from malicious or misbehaving endpoints.
- All shell-side HTTP consumers share a single source of network defaults.

### Negative

- `brioche-shell-runtime` now depends on `reqwest` directly (it already did transitively).
- A new cross-book ADR is required for any future change to the shared defaults.

### Neutral

- `StreamEvent::Error` is additive; existing providers that never emit it are unaffected.

## Affected Invariants

| Invariant | Impact |
|-----------|--------|
| I-Core-StreamNoBranch | **Upheld** — `dispatch_llm_stream` adds `Error` to the existing terminal-event match, no new branching logic. |
| I-Core-NoPanic | **Upheld** — `StreamEvent::Error` is included in property tests; no new `unwrap`/`expect` in Core. |
| I-Shell-Network-Signal | **Upheld** — failures continue to emit `SystemSignal::NetworkUnavailable` and now also emit a terminal `LlmStream` event. |
| I-Shell-Runtime-OnlyIO | **Upheld** — network I/O remains in the shell; Core performs no I/O. |
| I-Eco-OrderedCollections | **Upheld** — no persisted state changed; no `HashMap`/`HashSet` introduced. |

## Book References

- docs/SPECS.md §Book I — Kernel stream dispatch
- docs/SPECS.md §Book III-A — Shell runtime
- docs/SPECS.md §Book III-C — System tools
- docs/PHILOSOPHY.md §2.1 — Mechanism vs Policy in Code
- docs/PHILOSOPHY.md §3.4 — Documentation Standards
- CONTRIBUTING.md §Before Submitting PR — Cross-book changes require an ADR
