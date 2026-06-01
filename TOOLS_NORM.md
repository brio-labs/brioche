# Brioche Tool Authoring Standard

> A "brioche tool" is any crate that implements the `SystemTool` trait and registers into a `SystemToolExecutor`. This document defines the norm that every tool crate must follow to be considered part of the coherent Brioche ecosystem.

---

## 1. Philosophy

A tool is a **leaf node** in the architecture. It has one job, does it well, and knows nothing about:
- The kernel (`brioche-core`)
- Governance plugins
- Other tools
- The agent that uses it

A tool only knows:
- The `SystemTool` trait (from `brioche-shell-runtime`)
- Its own domain logic
- `tokio::io` for async I/O
- `serde_json` for argument parsing

---

## 2. Crate Structure

Every tool crate follows the exact same layout:

```
crates/tools/brioche-tool-<name>/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs          # trait impl + re-export
    ├── config.rs       # tool-specific configuration (optional)
    ├── schema.rs       # JSON schema helpers (optional)
    └── tests/
        └── integration_tests.rs
```

### 2.1 Crate naming

| Pattern | Example | Forbidden |
|---------|---------|-----------|
| `brioche-tool-<verb>` | `brioche-tool-readfile` | `brioche-tools-readfile` (plural) |
| `brioche-tool-<noun>` | `brioche-tool-shell` | `tool-readfile` (missing prefix) |
| `brioche-tool-<domain>` | `brioche-tool-web` | `brioche-readfile` (missing "tool-") |

Multi-word names use kebab-case: `brioche-tool-web-search`, not `brioche-tool-web_search`.

### 2.2 Cargo.toml template

```toml
[package]
name = "brioche-tool-readfile"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Read the contents of a text file for Brioche agents"

[lib]
name = "brioche_tool_readfile"
path = "src/lib.rs"

[dependencies]
brioche-shell-runtime = { workspace = true }
serde = { version = "1", features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["fs"] }
tokio-util = { workspace = true }
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
tokio-test = "0.4"
tempfile = "3"

[lints]
workspace = true
```

**Rules:**
- Only depend on `brioche-shell-runtime` (traits), never on `brioche-core` directly.
- Never depend on other tool crates.
- `tokio` features are scoped to what the tool actually needs (`fs`, `process`, `net`).
- `reqwest` is only for tools that do HTTP.

---

## 3. The `SystemTool` Contract

Every tool implements this trait, defined in `brioche-shell-runtime`:

```rust
#[async_trait::async_trait]
pub trait SystemTool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn run(
        &self,
        args: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<String, ToolError>;
}
```

### 3.1 Tool struct

```rust
/// Reads the contents of a text file.
pub struct ReadFileTool;
```

- Unit struct (`pub struct ReadFileTool;`) if stateless.
- Named struct with config fields if stateful (e.g. `ExecuteCommandTool` with sandbox policy).
- Always `#[derive(Clone, Debug)]` if stateful.
- Always `Default` if the default constructor exists.

### 3.2 Naming convention

| Element | Pattern | Example |
|---------|---------|---------|
| Struct | `<PascalCase>Tool` | `ReadFileTool`, `ExecuteCommandTool` |
| `name()` | `snake_case` verb | `"read_file"`, `"execute_command"` |
| `description()` | Imperative sentence | `"Read the contents of a text file."` |

### 3.3 Schema generation

Tools MUST use the standard OpenAI function-calling schema format:

```json
{
  "type": "function",
  "function": {
    "name": "read_file",
    "description": "Read the contents of a text file.",
    "parameters": {
      "type": "object",
      "properties": {
        "path": {
          "type": "string",
          "description": "Absolute or relative path to the file"
        }
      },
      "required": ["path"]
    }
  }
}
```

**Helper:** Place shared schema-building helpers in a `schema.rs` module or use a small macro. Do not inline raw `serde_json::Map` construction in every tool — it is error-prone and ugly.

### 3.4 Cancellation

Every tool MUST respect the `CancellationToken`:

```rust
async fn run(&self, args: serde_json::Value, cancel: CancellationToken) -> Result<String, ToolError> {
    let url = args["url"].as_str().ok_or_else(|| ToolError::InvalidArgs("missing 'url'".into()))?;

    let client = reqwest::Client::new();
    let request = client.get(url);

    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            return Err(ToolError::Io(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            )));
        }
        result = request.send() => result,
    };

    // ...
}
```

- `tokio::select! { biased; _ = cancel.cancelled() => ... }` at the top-level await.
- Do not check `cancel.is_cancelled()` at loop boundaries only — the user expects immediate cancellation.

### 3.5 Error handling

Tools return `Result<String, ToolError>`. The `SystemToolExecutor` maps these to `ToolResultDTO`:

| `ToolError` variant | Maps to | When to use |
|---------------------|---------|-------------|
| `SandboxDenied` | `ToolOutcome::BusinessError` | Policy blocked the tool |
| `InvalidArgs` | `ToolOutcome::BusinessError` | Required parameter missing or wrong type |
| `NotFound` | `ToolOutcome::BusinessError` | Target file/directory/resource does not exist |
| `Io` | `ToolOutcome::SystemError` | OS-level failure (permission denied, network timeout) |

**Rule:** Business errors (bad args, sandbox deny) are `BusinessError`. System errors (disk full, network unreachable) are `SystemError`.

---

## 4. Configuration Pattern

Tools that need configuration expose a builder or constructor:

```rust
pub struct ExecuteCommandTool {
    policy: SandboxPolicy,
    confirm_handler: Option<ConfirmHandler>,
}

impl ExecuteCommandTool {
    pub fn new() -> Self { /* ... */ }
    pub fn with_allow_list(list: AllowList) -> Self { /* ... */ }
    pub fn with_confirm_handler(mut self, handler: ConfirmHandler) -> Self { /* ... */ }
}
```

**Rules:**
- State is injected at construction time, never mutated after registration.
- Configuration types (`SandboxPolicy`, `AllowList`) live in `brioche-shell-runtime` if shared across tools, or in the tool crate if tool-specific.
- No `&mut self` in `SystemTool::run` — tools are `Send + Sync` and shared across concurrent calls.

---

## 5. Testing Requirements

Every tool crate MUST have integration tests:

```rust
#[tokio::test]
async fn read_file_reads_existing_file() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    tokio::fs::write(temp.path(), "hello").await.unwrap();

    let tool = ReadFileTool;
    let args = serde_json::json!({ "path": temp.path().to_str().unwrap() });
    let result = tool.run(args, CancellationToken::new()).await.unwrap();

    assert_eq!(result, "hello");
}

#[tokio::test]
async fn read_file_fails_on_missing_file() {
    let tool = ReadFileTool;
    let args = serde_json::json!({ "path": "/does/not/exist" });
    let result = tool.run(args, CancellationToken::new()).await;

    assert!(matches!(result, Err(ToolError::Io(_))));
}

#[tokio::test]
async fn read_file_respects_cancellation() {
    let tool = ReadFileTool;
    let args = serde_json::json!({ "path": "/dev/zero" });
    let cancel = CancellationToken::new();
    cancel.cancel(); // pre-cancelled

    let result = tool.run(args, cancel).await;
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}
```

**Mandatory test cases:**
1. Happy path — tool succeeds with expected output
2. Missing required argument — returns `InvalidArgs`
3. Resource not found — returns `Io` or `NotFound`
4. Cancellation — returns interrupted error when pre-cancelled
5. Schema validity — `parameters_schema()` returns valid JSON Schema

---

## 6. Documentation Requirements

Every tool crate MUST have:

### 6.1 `lib.rs` header
```rust
//! # Brioche Tool — Read File
//!
//! Reads the contents of a text file.
//!
//! ## Tool name
//! `read_file`
//!
//! ## Arguments
//! | Name | Type | Required | Description |
//! |------|------|----------|-------------|
//! | `path` | `string` | yes | Absolute or relative path to the file |
//!
//! ## Example
//! ```json
//! { "path": "/home/user/.bashrc" }
//! ```
//!
//! Refs: I-Shell-ToolResult-PassThrough
```

### 6.2 README.md
```markdown
# brioche-tool-readfile

Reads the contents of a text file for Brioche agents.

## Usage

```rust
use brioche_shell_runtime::SystemToolExecutor;
use brioche_tool_readfile::ReadFileTool;

let executor = SystemToolExecutor::new()
    .with_tool(ReadFileTool);
```

## Arguments

| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | `string` | yes | Path to the file |

## Safety

- No path traversal guards — the agent's sandbox policy is responsible for restricting paths.
- Reads the entire file into memory — large files may cause memory pressure.
```

---

## 7. Dependency Isolation

### 7.1 What a tool crate may depend on

| Dependency | When |
|------------|------|
| `brioche-shell-runtime` | Always (trait) |
| `serde`, `serde_json` | Always (argument parsing) |
| `tokio` | Always (async runtime) |
| `tokio-util` | When using `CancellationToken` |
| `thiserror` | Always (error definitions) |
| `tracing` | Always (structured logging) |
| `reqwest` | Only for HTTP tools |
| `russh` / `openssh` | Only for SSH tools |
| `redis` / `sqlx` | Only for DB tools |

### 7.2 What a tool crate MUST NOT depend on

- `brioche-core` — tools are trait implementations, not kernel code
- `brioche-governance` — tools don't make policy decisions
- Other tool crates — tools don't compose other tools (see §12)
- `brioche-std` — policies are injected by the agent, not hardcoded in tools

---

## 8. Shared Services (The Alternative to Tool Dependencies)

Tools MUST NOT depend on other tools. If two tools share code, extract a **service crate** instead.

### What is a service?

A service is infrastructure — not a tool. It has no `SystemTool` impl, no JSON schema, no `name()`. It is a library that tools use internally.

| Tool | Uses service |
|------|--------------|
| `fetch_url` | `brioche-service-http` (shared reqwest client, connection pool, retry policy) |
| `web_search` | `brioche-service-http` + `brioche-service-html` (scraping, extraction) |
| `read_file` | `tokio::fs` directly (no service needed — too simple) |
| `execute_command` | `brioche-service-process` (shared process spawner, sandbox wrapper) |

### Service crate layout

```
crates/services/brioche-service-http/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── client.rs      # Shared reqwest client with retry
    └── retry.rs       # Exponential backoff for HTTP
```

### Service rules

- Services live in `crates/services/`, not `crates/tools/`.
- Services have no `SystemTool` impl.
- Services are `pub` libraries with documented APIs.
- Multiple tools may depend on the same service.
- Services may depend on each other (e.g. `brioche-service-web-search` depends on `brioche-service-http`).

### Why not tool-to-tool?

| Problem | Example |
|---------|---------|
| **Circular dependencies** | `web_search` → `fetch_url` → `web_search` (to resolve relative URLs) |
| **Sandbox policy leakage** | Allowing `web_search` silently allows `fetch_url` — the user didn't consent |
| **Non-deterministic depth** | Tool A calls Tool B which calls Tool C — the LLM can't predict nesting |
| **Error attribution** | `web_search` fails — was it search logic or fetch logic? |
| **Schema explosion** | Tool A's schema must document Tool B's args too |
| **Testing complexity** | Mocking Tool B inside Tool A's tests requires a fake executor |

The subagent mechanism (Book II, `SubRoutineHandler`) is the correct way for the kernel to compose tools. A tool that needs another tool's behavior should:
1. Use a shared service for the common infrastructure, OR
2. Emit an `Effect::ExecuteTools` and let the kernel schedule the sub-tool, OR
3. Be redesigned as a single tool that does both things.

### Exception: Tool adapters

A "tool adapter" is a tool that wraps another tool with additional policy. For example:
- `brioche-tool-readfile-strict` wraps `ReadFileTool` with a path whitelist.
- This is NOT a tool-to-tool dependency — it is the same tool logic with different configuration.
- The adapter crate may re-export the base tool's struct and add a thin wrapper.

---

## 10. Feature Flags

Tools with heavy optional dependencies use feature flags:

```toml
[features]
default = []
full = ["sandbox", "confirm"]
sandbox = []
confirm = ["sandbox"]
```

**Rules:**
- `default = []` — the minimal viable tool compiles with no features.
- Features are additive only (never mutually exclusive).
- Feature names are nouns/adjectives, not verbs (`sandbox`, not `enable-sandbox`).

---

## 11. Registration in Agents

An agent composes tools at build time:

```rust
use brioche_shell_runtime::SystemToolExecutor;
use brioche_tool_readfile::ReadFileTool;
use brioche_tool_writefile::WriteFileTool;
use brioche_tool_shell::ExecuteCommandTool;

let tools = SystemToolExecutor::new()
    .with_tool(ReadFileTool)
    .with_tool(WriteFileTool)
    .with_tool(ExecuteCommandTool::with_allow_list(my_list));
```

**Rules:**
- The agent decides which tools to include.
- The agent configures tool-specific policies (allow-lists, timeouts).
- Tools never self-register or auto-discover.

---

## 12. Checklist for New Tool Authors

Before submitting a tool crate:

- [ ] Crate named `brioche-tool-<name>`
- [ ] Depends only on `brioche-shell-runtime` + domain-specific crates
- [ ] Implements `SystemTool` with correct naming
- [ ] Schema follows OpenAI function-calling format
- [ ] Respects `CancellationToken` via `tokio::select!`
- [ ] Errors use `ToolError` variants correctly
- [ ] Has integration tests (happy path, bad args, not found, cancellation, schema)
- [ ] `lib.rs` has `//!` header with tool name, arguments table, example
- [ ] README.md follows the standard template
- [ ] All `pub` items have doc comments with `Refs:`
- [ ] `cargo fmt`, `cargo clippy`, `cargo test` pass

---

## 13. Future: `brioche-tool-macro`

When the pattern stabilizes, a proc-macro may generate the boilerplate:

```rust
#[derive(SystemTool)]
#[tool(name = "read_file", description = "Read a text file.")]
struct ReadFileTool {
    #[tool_param(required = true, description = "Path to the file")]
    path: String,
}

#[async_trait]
impl ReadFileTool {
    async fn run(&self, cancel: CancellationToken) -> Result<String, ToolError> {
        tokio::fs::read_to_string(&self.path).await.map_err(ToolError::from)
    }
}
```

This macro would generate:
- The `SystemTool` impl
- The JSON schema
- The `serde_json::Value` argument extraction
- The cancellation wiring

**Not implemented yet.** Tools are written manually until the pattern is proven.
