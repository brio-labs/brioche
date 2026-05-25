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
| **Policy traits** | `*Policy`, `*Guard`, `*Handler` | `CycleBudgetPolicy`, `EpochGuard`, `SubRoutineHandler` | Signals "this is injectable policy." |
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
//! - I-Core-ExtO1: Extension access is O(1) by `TypeId`.
//!
//! ## Entry points
//! - `BriocheEngine::transition()`: Main state transition function.
//! - `ExtensionStorage`: Type-safe, O(1) plugin state container.
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
- Hot path functions document complexity/budget (via `scripts/check_hotpath_docs.py`)

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

## 7. Summary: The Non-Negotiables

| Rule | Enforcement |
|------|-------------|
| **No OOP inheritance.** Traits only. | Compiler + Clippy |
| **No `HashMap`/`HashSet` in persisted state.** | `BriocheExtensionType` proc-macro + `clippy` |
| **No hidden side effects.** All effects in `Vec<Effect>`. | Code review + architecture review |
| **No panics in Core.** | `clippy` deny `unwrap`/`expect` |
| **All `pub` items documented with invariant refs.** | `RUSTDOCFLAGS="-D warnings"` + bot check |
| **Hot paths document complexity.** | PR checklist + `scripts/check_hotpath_docs.py` |
| **Mechanism vs Policy separation.** | Human review + ADR requirement |
| **Determinism by design, not by test.** | `proptest` + replay tests + `HashMap` ban |

This philosophy is not a suggestion. It is the **immune system** of the codebase. Violate it, and the architecture rots. Enforce it, and the compiler becomes your strictest, most reliable collaborator.
