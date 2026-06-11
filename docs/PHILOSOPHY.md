# The Brioche Code Philosophy & Style Canon

> **This document is enforceable.** Violations block CI. No exceptions.

---

## 1. Paradigm: Reject OOP Inheritance, Embrace Algebraic Composition

### Why SOLID (as traditionally taught) is dangerous for Brioche

SOLID was forged for mutable class hierarchies in garbage-collected languages. Brioche is a deterministic, zero-allocation, synchronous kernel. Traditional OOP patterns **directly threaten** our invariants:

| OOP Pattern | Why it fails in Brioche | Brioche Replacement |
|-------------|------------------------|---------------------|
| **Inheritance** | Creates hidden coupling between layers. A `BasePlugin` change ripples unpredictably. Violates *Extension over Modification*. | **Trait composition.** Each capability is a standalone trait. Combine via `impl TraitA + TraitB`. |
| **Polymorphic dispatch (vtables)** | Runtime indirection in the hot path. Cache-unfriendly. Violates *I-Core-StreamNoBranch*. | **Pre-routing tables.** `UnifiedRoutingTable` resolves at init time. O(1) index access, not vtable traversal. |
| **Encapsulation via private state** | Hides determinism killers (e.g., a `HashMap` inside a "private" field). Violates *I-Eco-OrderedCollections*. | **Type transparency + proc-macros.** `BriocheExtensionType` makes determinism properties visible to the compiler. |
| **Dependency Injection (runtime)** | Runtime resolution is non-deterministic and panics if unbound. | **Builder injection.** `BriocheEngineBuilder` wires traits at compile time. Missing trait = compile error. |

### The Brioche Paradigm

We practice **Algebraic Systems Design**:

1. **Data first, behavior second.** Define your state as precise ADTs (`enum`, `struct`) before writing functions.
2. **Types are the spec.** If a state transition is illegal, make it unrepresentable (`AgentState::Predicting` requires a `generation_id` in the type, not a runtime check).
3. **Traits are capabilities, not taxonomies.** A trait says "you can do X," never "you are a Y."
4. **Effects are explicit.** All side effects must be named, typed, and returned in `Vec<Effect>`. No hidden I/O.

---

## 2. The Brioche Creed (Non-Negotiable Principles)

Every contributor must internalize these. They override "clean code" aesthetics.

### 2.1 Mechanism vs Policy in Code
- **Mechanism code** (Core) is boring, explicit, and minimal. It handles state transitions, routing, and memory layout.
- **Policy code** (Governance, Plugins) is where business logic lives. It never modifies mechanism types.
- **Rule:** If you find an `if` branch in Core that checks a business rule (quarantine, timeout, user tier), you have violated the separation. Move it to a governance plugin.

### 2.2 Determinism is a Static Property
- Determinism is not tested; it is **designed**.
- No `rand`, no `HashMap` in persisted state, no `Instant::now()` in Core, no thread-local storage.
- The compiler must be able to prove that two identical inputs produce identical outputs.

### 2.3 The Hot Path is Sacred
- `handle_stream_event`, `ExtensionStorage::get_mut`, `HookEffectConstraint::is_allowed_fast` are sacred.
- Before optimizing, measure. After optimizing, document the budget.
- **Rule:** Any function called inside `transition()` must have a documented complexity and allocation contract.

### 2.4 Panics are Bugs, Never Features
- A panic in Core is a kernel crash. It is never caught.
- Use `Result` for recoverable errors. Use `enum` variants for expected failure modes (`Failure` state).
- `unwrap()`, `expect()`, `unreachable!()` are forbidden by `clippy` (deny level).

### 2.5 Explicit > Clever > Concise
- Prefer `match` over combinators when branching logic is complex.
- Prefer named intermediate variables over long method chains.
- **Verbosity is acceptable if it preserves clarity of invariant.**

---

## 3. Code Style Standards

### 3.1 Formatting (Enforced by `rustfmt.toml`)

```toml
# rustfmt.toml
edition = "2024"
max_width = 100
tab_spaces = 4
use_small_heuristics = "Default"
imports_granularity = "Module"        # One use statement per module
group_imports = "StdExternalCrate"    # std -> external -> crate
reorder_impl_items = true             # Keep trait impls ordered
format_code_in_doc_comments = true
```

### 3.2 Naming Conventions

| Context | Convention | Example | Rationale |
|---------|-----------|---------|-----------|
| **Mechanism types** | Noun, precise, mechanical | `Session`, `AgentState`, `ExtensionStorage` | These are the atoms of the system. |
| **Policy traits** | `*Policy`, `*Guard`, `*Handler` | `EpochGuard`, `SubRoutineHandler` | Signals "this is injectable policy." |
| **Plugin structs** | Descriptive noun | `QuarantineManager`, `ToolCallDetector` | Not `PluginX`. The trait already says it's a plugin. |
| **Effect variants** | Imperative verb | `CallLlmNetwork`, `ExecuteTools`, `SaveSession` | Effects are commands. |
| **State extensions** | `*State` | `EpochState`, `QuarantineState` | Always ends in State. Stored in `ExtensionStorage`. |
| **Error types** | `*Error` | `PluginError`, `BriocheError` | Never `Exception`, never `Failure` (reserved for state). |
| **Boolean flags** | Positive assertion | `is_allowed`, `has_timed_out` | Never `is_not_allowed`. |

### 3.3 Function Design Rules

#### Rule: The "No Surprises" Signature
Every function must declare its contract in its signature.

```rust
// GOOD: Contract is explicit
/// Validates effect permission in O(1) via bitmask.
///
/// # Invariants
/// - I-Core-HookEffect-O1
///
/// # Complexity
/// O(1). No heap allocation.
fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;

// BAD: What does it return? What are the side effects?
fn check(hook: u8, eff: u64) -> bool;
```

#### Rule: The `mut` Boundary
- `&mut` means "this function may modify the session or extensions."
- If a function takes `&mut Session` but only reads, change it to `&Session`.
- If a function modifies `ExtensionStorage` but not `Session`, take `&mut ExtensionStorage` explicitly, not `&mut Session`.

#### Rule: Error Propagation
Use `PluginResult<T>` (aliased `Result<T, PluginError>`) for plugin hooks. Use `BriocheError` for system failures.

```rust
pub type PluginResult<T> = Result<T, PluginError>;

// Never use `?` to convert between error types implicitly.
// Explicit mapping preserves invariant context.
```

### 3.4 Control Flow

#### Forbidden Patterns
```rust
// FORBIDDEN: Hidden non-determinism
let map: HashMap<_, _> = items.collect(); // In governance/plugin code

// FORBIDDEN: Implicit panic
let val = vec[0]; // Use vec.first() or match on len

// FORBIDDEN: Magic priority numbers
fn priority(&self) -> i16 { 42 } // Must be a named constant or documented

// FORBIDDEN: Nested match hell
match x {
    A => match y { ... },
    _ => ...
}
// Flatten with early returns or helper functions.
```

#### Required Patterns
```rust
// REQUIRED: Exhaustive matching on mechanical enums
match state {
    AgentState::Idle => ...,
    AgentState::Predicting { generation_id } => ...,
    AgentState::ExecutingTools { generation_id } => ...,
    AgentState::SubRoutine(handle) => ...,
    AgentState::Failure => ...,
}

// REQUIRED: Explicit epoch check before sub-routine handling
// (I-Comp-Epoch-Subroutine)
let epoch_action = epoch_guard.intercept_epoch(input, ext)?;
if let EpochAction::Block { reason } = epoch_action {
    return Ok(vec![Effect::Error { ... }]);
}
// Now safe to call subroutine_handler...
```

---

## 4. Documentation Standards

Documentation is not prose. It is **a contract between the code and the invariant system.**

### 4.1 Mandatory Documentation Blocks

Every `pub` item MUST have:

1. **One-sentence purpose.** What does this do?
2. **Invariant references.** Which system invariants does this uphold or rely on? Format: `Refs: I-Category-Name`
3. **Complexity / Budget.** For hot path items: time and space complexity. For async items: blocking behavior.
4. **Panic / Safety contract.** When can this panic? What preconditions must hold?

```rust
/// Intercepts the current epoch to enforce temporal isolation.
///
/// This is the first governance trait evaluated in every transition cycle.
/// No subsequent trait may override an epoch barrier.
///
/// # Invariants
/// - I-Comp-Epoch-First: Always evaluated before other traits.
/// - I-Comp-Epoch-Subroutine: Short-circuits `SubRoutineHandler` on past epoch.
/// - I-Gov-Epoch-Reject: Silently rejects asynchronous responses with stale epochs.
///
/// # Complexity
/// O(1). Reads `EpochState` from `ExtensionStorage` via `get_or_insert_default`.
///
/// # Errors
/// Returns `PluginError::Soft` if `ExtensionStorage` access fails.
/// Never returns `Fatal`; epoch rejection is a silent `Block`.
pub trait EpochInterceptor: Send + Sync {
    fn intercept_epoch(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> Result<EpochAction, PluginError>;
}
```

### 4.2 Inline Comments: Explain the "Why," Never the "What"

```rust
// BAD: What the code does (obvious from reading)
// Increment the epoch
epoch_state.current_generation += 1;

// GOOD: Why it exists (invariant preservation)
// Increment epoch to orphan any in-flight async tasks from previous cycles.
// This satisfies I-Gov-Epoch-Reject: stale epochs are rejected on next input.
epoch_state.current_generation += 1;
```

### 4.3 Module-Level Documentation (`//!`)

Every crate root and module must have a `//!` block explaining:
- Its position in the 5-Book architecture.
- Its public interface surface.
- What invariants it guarantees.

```rust
//! # Brioche Core — Book I
//!
//! The synchronous kernel. Never performs I/O. Computes transitions
//! from `(Session, EngineInput)` to `Vec<Effect>`.
//!
//! ## Invariants upheld
//! - I-Core-Pure: No side effects.
//! - I-Core-NoPanic: Anomalies produce `Failure` state or `OverrideTransition`.
//! - I-Core-ExtO1: Extension access is O(log n) by `TypeId` (n = registered types, typically < 20).
//!
//! ## Entry points
//! - `BriocheEngine::transition()`: Main state transition function.
//! - `ExtensionStorage`: Type-safe, O(log n) plugin state container.
```

### 4.4 Architecture Decision Records in Code

For complex algorithms, embed the rationale:

```rust
/// Granular COW snapshot mechanism.
///
/// ## Why not full snapshot?
/// Full `ExtensionStorage` clone on every hook would be O(n) where n = total
/// extensions. With 26 standard governance plugins, this exceeds hot path
/// budget (I-Core-StreamNoBranch). Instead, we clone only on first write
/// (O(k) where k = mutated extensions).
///
/// ## Why `clone_box` via VTable?
/// `ExtensionStorage` is type-erased (`Box<dyn Any>`). We cannot call
/// `Clone::clone` directly. The `ExtVTable` provides a function pointer
/// generated by `BriocheExtensionType` proc-macro.
///
/// ## Rollback is best-effort
/// If cumulative weight exceeds `max_cow_bytes_per_hook`, we abandon
/// restoration to avoid synchronous allocation spikes.
/// See I-Gov-Rollback-BestEffort.
pub struct UndoFrameState { ... }
```

---

## 5. Enforcement: Making Philosophy Inescapable

Philosophy without enforcement is wishful thinking. We use the compiler, CI, and review bots as enforcers.

### 5.1 Compiler-Level Enforcement (`clippy.toml`)

```toml
# clippy.toml
avoid-breaking-exported-api = false

# Brioche-specific lints
disallowed-methods = [
    # Determinism killers
    "std::collections::HashMap::new",
    "std::collections::HashSet::new",
    "std::time::Instant::now",        # Only in shell, never in core

    # Panic sources
    "std::option::Option::unwrap",
    "std::result::Result::unwrap",
    "std::vec::Vec::swap_remove",     # Non-deterministic ordering
]

disallowed-types = [
    "std::collections::HashMap",
    "std::collections::HashSet",
    # HashMap is allowed ONLY in transient, non-persisted caches.
    # Use #[allow(clippy::disallowed_types)] with a mandatory comment.
]
```

### 5.2 CI Enforcement

See `.github/workflows/ci.yml` for the `philosophy-check` job which enforces:
- All `pub` items have doc comments (`RUSTDOCFLAGS="-D warnings"`)
- Invariant references use proper `Refs:` format
- No `unwrap`/`expect`/`panic!` in `brioche-core`
- Hot path functions document complexity/budget (via `scripts/philosophy-check.py`)

### 5.3 Pull Request Bot Checklist

Auto-posted by a GitHub Action on every PR:

```markdown
## Brioche Philosophy Checklist (Bot)

- [ ] No `HashMap`/`HashSet` in persisted state (unless explicitly exempted)
- [ ] All `pub` items have doc comments with `Refs: I-...`
- [ ] Hot path functions document complexity/budget
- [ ] No `unwrap`/`expect` in `brioche-core`
- [ ] No business logic in mechanism code (Core)
- [ ] Trait implementations are atomic (no inheritance-like coupling)
- [ ] ADR linked if crossing book boundaries
```

### 5.4 Code Review Human Checklist

Reviewers must verify:

1. **Does this change uphold or violate any invariant?** If it changes behavior, the spec must be updated.
2. **Is this mechanism or policy?** If policy, it must be in a plugin/trait, not in Core.
3. **Where is the `proptest`?** New state machines require property tests.
4. **Is the documentation lying?** Check that doc comments match code behavior.

### 5.5 Crate Categories and Applied Standards

Not every rule applies with the same strictness to every crate. The workspace is organized by architectural book, and the `philosophy-check` script maps checks accordingly.

| Book | Crates | Mandatory checks |
|------|--------|------------------|
| **Book I — Core** | `brioche-core`, `brioche-macro` | All §5 checks; determinism guards; no panics; no hidden I/O |
| **Book II — Governance** | `brioche-governance`, `brioche-governance-default` | All §5 checks; trait-hierarchy guard |
| **Book III-A — Shell Runtime** | `brioche-shell-runtime`, `brioche-shell-persistence`, `brioche-shell-projection` | Module docs; invariant refs; English prose; TODO policy; async standards (§10) |
| **Book III-B — Providers** | `brioche-provider-openai`, future providers | Module docs; invariant refs; English prose; TODO policy; structured errors (§12) |
| **Book III-C — Tools** | `brioche-tools-system`, future tools | Module docs; invariant refs; English prose; TODO policy |
| **Book IV — Apps** | `agent-terminal`, future apps | Module docs; English prose; TODO policy; no `println!` in library modules; CLI exit conventions |
| **Infrastructure** | `cargo-brioche-lint*`, `brioche-reedline`, `brioche-docgen`, `brioche-plugin-kit`, `brioche-std`, `brioche-playground` | Module docs; invariant refs where applicable; English prose; TODO policy |

Crates in Books III-A through Infra are exempt from determinism guards (`HashMap` ban, `Instant::now` ban, etc.) because they legitimately perform I/O, use caches, and sample clocks. They are **not** exempt from documentation, language, or TODO standards.

---

## 6. Example: The "Brioche Way" in Practice

### Task: Add a new governance plugin `RateLimiter`

**Wrong approach (OOP thinking):**
```rust
// BAD: Inherits from a base, hides state, uses HashMap for "performance"
pub struct RateLimiter {
    limits: HashMap<String, u32>, // VIOLATION: I-Eco-OrderedCollections
}

impl BaseGovernancePlugin for RateLimiter { ... } // VIOLATION: No inheritance
```

**Correct approach (Brioche Way):**
```rust
// GOOD: Explicit state, ordered collection, standalone trait impl
use brioche_macro::BriocheExtensionType;

/// Limits user message frequency by policy.
///
/// # Invariants
/// - I-Eco-ExtensionOverMod: Rate limiting is policy, not mechanism.
/// - I-Eco-OrderedCollections: Uses `BTreeMap` for deterministic iteration.
///
/// # Decision Strategy
/// Returns `Block` if `last_message_ts + cooldown_ms > now`.
/// `now` is provided by the shell via `SystemSignal::Tick`, never
/// sampled directly in the plugin to preserve determinism.
#[derive(BriocheExtensionType)]
pub struct RateLimitState {
    /// Map user_id -> last_message_timestamp_ms.
    /// BTreeMap ensures deterministic iteration order.
    pub last_message_ts: BTreeMap<String, u64>,
    pub cooldown_ms: u64,
}

pub struct RateLimiter {
    cooldown_ms: u64,
}

impl BriochePlugin for RateLimiter {
    fn name(&self) -> &'static str { "rate_limiter" }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 { 100 } // Early interceptor

    fn owned_state_keys(&self) -> &'static [&'static str] {
        &["rate_limiter::state"]
    }

    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let state = ext.get_or_insert_default::<RateLimitState>();

        // Policy decision: block if cooldown active
        match input {
            EngineInput::UserMessage(_) => {
                // Implementation...
                Ok(PolicyDecision::Allow)
            }
            _ => Ok(PolicyDecision::Allow),
        }
    }
}
```

---

## 7. Compositional Design Canon: CUPID, SCIFI, Functional Programming, and Data-Oriented Design

Brioche's rejection of SOLID is not an absence of philosophy—it is an affirmative choice for a different family of principles. The following methodologies provide the theoretical foundation that validates our architectural decisions and keeps the codebase simple, maintainable, and auditable.

---

### 7.1 CUPID: The Properties of Joyful Code

Dan North's CUPID framework defines five properties that make code *maintainable* without rigid rules. Brioche maps each property to an architectural commitment:

| CUPID Property | Brioche Manifestation |
|----------------|----------------------|
| **Composable** | `BriochePlugin` trait + `UnifiedRoutingTable`. Plugins assemble without structural coupling. Traits are capabilities, not taxonomies. |
| **Unix philosophy** | Mechanism vs Policy separation. Each plugin does one thing (`EpochGuard`, `DepthGuard`, `QuarantineManager`). No plugin builds UI JSON *and* checks depth limits. |
| **Predictable** | Determinism by design: `BTreeMap`, no `rand`, no `Instant::now()` in Core. Effects are explicit. Two identical inputs always produce identical outputs. |
| **Idiomatic** | Sealed traits, proc-macros, exhaustive `match`, `Result` for errors—native Rust patterns. No OOP-in-Rust cargo culting. |
| **Domain-based** | `AgentState`, `EngineInput`, `Effect`—names directly model the LLM orchestration domain. The code reads like the spec. |

### 7.2 SCIFI: Compositional Design as Methodology

Jakob Jenkov's SCIFI principles provide an operational workflow for composition-driven design. Brioche follows them explicitly:

1. **Split** — Divide responsibilities into small, independent units. Each governance plugin is a single observable concern. `ToolCallDetector` only counts; `ToolResultFormatter` only formats.
2. **Connect** — Wire units through explicit interfaces. `BriocheEngine` + `UnifiedRoutingTable` connect plugins without them knowing each other exist.
3. **Improve** — Refine interfaces until they are self-documenting. `PluginCapabilities` evolved from a runtime bitmask check into pre-routed indices (I-Core-StreamNoBranch).
4. **Facilitate** — Provide façades for common operations. `BriocheEngineBuilder` is the façade; governance profiles (`Permissive`, `Standard`, `Strict`) are pre-configured façades.
5. **Iterate** — Governance plugins are swapped without kernel changes. New safety policies are added via composition, never by modifying mechanism code.

### 7.3 Functional Programming: Data and Behavior Separation

Functional Programming eliminates inheritance by separating state from behavior. Brioche applies this at the architectural level:

- **State is data.** `AgentState`, `Session`, `ExtensionStorage` contain no logic. They are containers shaped by ADTs.
- **Behavior is functions.** Plugins are pure functions from `(input, state)` to `(decision, effects)`. The only "mutation" is explicit `&mut ExtensionStorage`.
- **Effects are values.** `Vec<Effect>` is a data structure describing intent, not an action. The shell interprets it.
- **No hidden I/O.** Every side effect is named, typed, and returned. There are no `async fn` in Core, no background threads, no implicit logging.

### 7.4 Data-Oriented Design: Layout First, Logic Second

Data-Oriented Design (DOD) optimizes for memory layout and cache locality. In Brioche:

- **Entities are identifiers.** A `Session` is an ID that collects components (`AgentState`, `ExtensionStorage`, `history`). Systems (plugins) process them independently.
- **Behavior-free data.** `BriocheExtensionType` types carry no methods. They are plain data structs processed by plugin systems.
- **Cache-conscious access.** `ExtensionStorage` uses `BTreeMap` (tree of contiguous nodes) rather than `HashMap` (pointer-chasing buckets). For *n* < 20 registered types, tree traversal is cache-friendlier than hash indirection.
- **Pre-computed dispatch.** `UnifiedRoutingTable` resolves plugin routes at init time, eliminating hot-path branching and vtable traversal.

### 7.5 Design Rules Derived from the Canon

The following rules operationalize CUPID/SCIFI/FP/DOD for day-to-day Brioche development. Violations block CI.

#### Rule: One Concern Per Plugin (Unix Philosophy)
A plugin must do exactly one observable thing. If you find yourself writing "and" in the plugin's doc comment, split it.

```rust
// BAD: DepthGuard calculates depth AND builds UI effects AND formats error JSON
// GOOD: DepthGuard calculates depth and returns PolicyDecision::Block
//       The UI projection layer handles rendering ErrorCode::StateInconsistency
```

#### Rule: Extract Pure Functions from Hooks (FP)
Move deterministic calculation logic out of hook methods into standalone pure functions. This makes the logic unit-testable without `ExtensionStorage` mocks.

```rust
// GOOD: Pure function, testable without mocks
fn calculate_depth(stack_depth: u64, current_state: AgentStateTag) -> u64 {
    if current_state == AgentStateTag::SubRoutine { stack_depth + 1 } else { stack_depth }
}

// In the hook:
let depth = calculate_depth(snapshot.state_stack_depth, snapshot.current_state);
```

#### Rule: No Stringly-Typed Holes in Effect (Domain-Based)
`Effect` variants must carry structured data, not `String` or `serde_json::Value` umbrellas. If the domain has known widget types, model them as an enum.

```rust
// BAD: Untyped payload — the compiler cannot help you audit this
Effect::ForwardToUi { widget_type: String, payload: serde_json::Value }

// GOOD: Structured, exhaustively matchable — the compiler is your auditor
Effect::ForwardToUi(UiWidget::Error { code: String, message: String })
```

#### Rule: Traits Are Capabilities, Not Taxonomies (Composable)
A trait must declare "I can do X," never "I am a Y." No supertrait hierarchies. No `BasePlugin`.

```rust
// BAD: Taxonomy trait — creates inheritance coupling
trait GovernancePlugin: BriochePlugin { ... }

// GOOD: Capability trait — standalone, composable
trait EpochInterceptor: Send + Sync { ... }
```

#### Rule: Document the Data Layout (DOD)
When adding a new `BriocheExtensionType`, document its memory footprint and snapshot strategy. The reviewer must verify cache friendliness.

```rust
/// COW snapshot strategy: FullClone (< 256 bytes).
/// Estimated weight: 128 bytes (two BTreeMaps, typically < 10 entries each).
#[derive(BriocheExtensionType)]
pub struct QuarantineState { ... }
```

---

## 9. Testing Canon

Untested code is unreviewable code. Every crate category carries a minimum test obligation.

| Category | Unit tests | Property tests | Integration tests | Benchmarks |
|----------|------------|----------------|-------------------|------------|
| **Book I — Core** | Required for all `pub` functions | Required for state machines and transitions | Required for serialization round-trips | Required for hot-path functions |
| **Book II — Governance** | Required for plugin hooks | Strongly encouraged for policy decisions | Required for multi-plugin interaction | Encouraged for negotiation paths |
| **Book III-A — Shell Runtime** | Required for pure helpers | Encouraged | Required for async effect loops | Encouraged for persistence paths |
| **Book III-B — Providers** | Required for parsing/serialization | — | Mock-server tests required; no network in `cargo test` | — |
| **Book III-C — Tools** | Required | — | Required for idempotency and sandboxing | — |
| **Book IV — Apps** | Required for config/bridge logic | — | Smoke tests for headless mode | — |
| **Infrastructure** | Required for lint logic | — | Snapshot tests encouraged | — |

A crate with zero tests is not ready for `main`.

---

## 10. Async & Shell Code Standards

Books III-A, III-B, III-C, and Book IV have additional rules that do not apply to the synchronous kernel.

### 10.1 Cancellation Safety
Every `pub async fn` must document its cancellation contract. If the future is dropped mid-await, what invariants hold? What leaks?

```rust
/// Reads the next chunk from the SSE stream.
///
/// # Cancel safety
/// This future holds no locks across await points. Dropping it leaks
/// the underlying TCP connection, which is recovered by the connection
/// pool timeout.
pub async fn next_chunk(&mut self) -> Result<Chunk, ShellError>;
```

### 10.2 Channels and Backpressure
All internal channels must be bounded. Document the capacity and `DropPolicy`.

### 10.3 Library Code Never Exits
`std::process::exit` is permitted **only** in `main()`, `headless::run()`, or equivalent top-level CLI dispatch. Library crates must return `Result` and let the caller decide.

### 10.4 Output Conventions
- `println!` and `eprintln!` are allowed **only** in app crates.
- Library crates use `tracing` at the appropriate level.
- Never emit user-facing text from `brioche-core`, `brioche-governance`, or provider internals.

### 10.5 Error Mapping at Boundaries
When an async provider or tool returns an error, map it to the appropriate crate error type (`ShellError`, `PersistenceError`, etc.) at the architectural boundary. Never let provider-specific error types leak into `Effect` payloads.

---

## 11. TODO / FIXME Policy

`TODO` and `FIXME` are not free passes to merge half-finished work.

### Rules
1. **Kernel crates (`crates/kernel/*`)**: `TODO` and `FIXME` are forbidden in production code on `main`.
2. **Outer crates**: allowed only with an explicit attribution:
   - `TODO(Sprint N): ...` — scheduled work
   - `TODO(#issue): ...` — linked issue
   - `TODO(your-name): ...` — owner named
3. **Bare `TODO` / `FIXME`** without attribution is a CI failure.
4. **Stale TODOs** must be removed or converted to issues within one release cycle.

---

## 12. Cross-Crate Error Taxonomy

Brioche uses a layered error taxonomy. Each layer owns its error type and maps at boundaries.

| Layer | Error type | Purpose |
|-------|------------|---------|
| **Kernel** | `BriocheError` | Deterministic, serializable failure inside `transition()` |
| **Governance** | `PluginError` | Recoverable or fatal failure inside a plugin hook |
| **Shell Runtime** | `ShellError` | Async runtime failure (channel, I/O, effect execution) |
| **Persistence** | `PersistenceError` | Disk/database failure outside the kernel |
| **Providers** | Provider-specific (e.g., `OpenAiError`) | HTTP/network/model failure; must map to `ShellError` before crossing into `Effect` |
| **Tools** | Tool-specific | Execution failure; must map to a structured `ToolOutcome` |

### Mapping rules
1. Never leak a provider error into an `Effect` variant.
2. Never construct `BriocheError` outside `brioche-core`.
3. Async code returns `Result<T, ShellError>` (or a domain-specific error) and maps to `Effect::Error` at the shell boundary.

---

## 8. Summary: The Non-Negotiables

Additional rules from §9 through §12 are summarized below alongside the core canon.

| Rule | Enforcement |
|------|-------------|
| **No OOP inheritance.** Traits only. | Compiler + Clippy |
| **No `HashMap`/`HashSet` in persisted state.** | `BriocheExtensionType` proc-macro + `clippy` |
| **No hidden side effects.** All effects in `Vec<Effect>`. | Code review + architecture review |
| **No panics in Core.** | `clippy` deny `unwrap`/`expect` |
| **All `pub` items documented with invariant refs.** | `RUSTDOCFLAGS="-D warnings"` + bot check |
| **Hot paths document complexity.** | PR checklist + `scripts/philosophy-check.py` |
| **Mechanism vs Policy separation.** | Human review + ADR requirement |
| **Determinism by design, not by test.** | `proptest` + replay tests + `HashMap` ban |
| **One concern per plugin.** No plugin does two observable things. | Code review + PHILOSOPHY.md §7.5 |
| **Extract pure functions from hooks.** Testable without mocks. | Code review + unit-test gate |
| **No stringly-typed holes in `Effect`.** Structured payloads only. | Compiler + code review |
| **Traits are capabilities, not taxonomies.** No supertrait hierarchies. | Compiler + code review |
| **Document data layout.** Memory footprint and snapshot strategy. | Code review + `scripts/philosophy-check.py` |
| **English-only prose in doc comments.** | Code review + `scripts/philosophy-check.py` |
| **No bare `TODO`/`FIXME`; none at all in kernel crates.** | Code review + `scripts/philosophy-check.py` |
| **`pub async fn` documents cancel safety.** | Code review |
| **Library code returns errors; apps handle exit.** | Code review |
| **Tests exist for every crate.** | Code review + CI gate |
| **Provider/tool errors map at shell boundary.** | Code review |
| **`println!`/`eprintln!` only in app crates.** | Code review + `scripts/philosophy-check.py` |

This philosophy is not a suggestion. It is the **immune system** of the codebase. Violate it, and the architecture rots. Enforce it, and the compiler becomes your strictest, most reliable collaborator.
