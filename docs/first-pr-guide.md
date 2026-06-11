# First PR Guide: Contributing Your First Plugin to Brioche

> **Prerequisite:** Read [`PHILOSOPHY.md`](./PHILOSOPHY.md) before this guide. If any concept here is unclear, the Canon is the source of truth.

This guide walks you through a minimal, real contribution: adding a `PingPlugin` that responds to a `SystemSignal::Tick` with a `LogDebug` effect. No business logic — just enough to experience the full workflow.

---

## 1. Understand Where Your Change Lives

Brioche is organized into **5 Books** (architectural layers):

| Book | Crate(s) | What goes here |
|------|----------|----------------|
| I — Core | `brioche-core` | Kernel, state transitions, `ExtensionStorage`. **You do not modify this.** |
| II — Governance | `brioche-governance`, `brioche-governance-default` | Traits and plugin implementations. **Your plugin lives here.** |
| III — Shell | `brioche-shell-*` | I/O, persistence, networking, UI. **You do not modify this.** |
| IV — Ecosystem | `brioche-std`, `plugin-kit` | Standard utilities and SDK. |
| V — Tooling | `brioche-docgen`, `playground` | Build tools, docs, demos. |

**Rule:** If you are adding a new capability, it is almost always Book II (Governance).

---

## 2. Read the Relevant Spec

Before writing code, check if a spec already exists:

```bash
ls docs/architecture/
# book-i-core.md
# book-ii-governance.md
# ...
```

If your plugin introduces a new trait or modifies an existing one, the spec must be updated **before** code review. A PR that changes behavior without updating the spec is rejected.

---

## 3. Define the State (Data First)

In Brioche, you define your state as an ADT **before** you write any behavior.

Create `crates/brioche-governance-default/src/ping_plugin.rs`:

```rust
//! # PingPlugin — A minimal example of the Brioche Way.
//!
//! Demonstrates:
//! - `BriocheExtensionType` for deterministic persisted state.
//! - Trait composition (no inheritance).
//! - Explicit effects (no hidden I/O).
//!
//! ## Invariants
//! - I-Eco-OrderedCollections: Uses `Vec` as a FIFO stack.
//! - I-Eco-ExtensionOverMod: Ping logic is policy, not mechanism.

use brioche_core::{BriocheExtensionType, ExtensionStorage};
use brioche_governance::{BriochePlugin, PluginCapabilities, PluginResult, PolicyDecision};
use brioche_macro::BriocheExtensionType;

/// Persisted state for the ping plugin.
///
/// Refs: I-Eco-OrderedCollections
#[derive(BriocheExtensionType, Default)]
pub struct PingState {
    /// Number of ticks received since session start.
    pub tick_count: u64,
}

/// A minimal governance plugin that counts shell ticks.
///
/// # Invariants
/// - I-Eco-ExtensionOverMod: This is policy, not mechanism.
pub struct PingPlugin;

impl BriochePlugin for PingPlugin {
    fn name(&self) -> &'static str {
        "ping"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        // Low priority — runs after critical interceptors.
        10
    }

    fn owned_state_keys(&self) -> &'static [&'static str] {
        &["ping::state"]
    }

    /// Handles incoming engine input.
    ///
    /// # Complexity
    /// O(1). Single integer increment, no heap allocation.
    ///
    /// # Invariants
    /// - I-Eco-ExtensionOverMod: Only reads/writes `ExtensionStorage`, never `Session`.
    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let state = ext.get_or_insert_default::<PingState>();

        match input {
            EngineInput::SystemSignal(SystemSignal::Tick) => {
                state.tick_count += 1;
                Ok(PolicyDecision::Allow)
            }
            _ => Ok(PolicyDecision::Allow),
        }
    }
}
```

**Key observations:**
- `PingState` derives `BriocheExtensionType` — this makes it serializable and deterministic.
- No `HashMap`. No `unwrap`. No `Instant::now()`.
- The plugin never touches `Session` directly. It only mutates its own state in `ExtensionStorage`.

---

## 4. Register the Plugin

Add your module to `crates/brioche-governance-default/src/lib.rs`:

```rust
pub mod ping_plugin;
```

If there is a plugin registry (e.g., a `default_plugins()` function), add `PingPlugin` there too.

---

## 5. Write a Test

Every plugin needs a test. Create `crates/brioche-governance-default/src/ping_plugin.rs` (append at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use brioche_core::ExtensionStorage;

    #[test]
    fn ping_plugin_counts_ticks() {
        let plugin = PingPlugin;
        let mut ext = ExtensionStorage::new();

        let input = EngineInput::SystemSignal(SystemSignal::Tick);
        let _ = plugin.on_input(&input, &mut ext).unwrap();
        let _ = plugin.on_input(&input, &mut ext).unwrap();

        let state = ext.get::<PingState>().unwrap();
        assert_eq!(state.tick_count, 2);
    }

    #[test]
    fn ping_plugin_ignores_non_tick_input() {
        let plugin = PingPlugin;
        let mut ext = ExtensionStorage::new();

        let input = EngineInput::UserMessage("hello".to_string());
        let _ = plugin.on_input(&input, &mut ext).unwrap();

        let state = ext.get::<PingState>().unwrap();
        assert_eq!(state.tick_count, 0);
    }
}
```

---

## 6. Run the Checks

Before committing, run the full local validation:

```bash
# Formatting
cargo fmt -- --check

# Linting (includes philosophy checks)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Dependency audit
cargo deny check all

# Tests
cargo test --workspace

# Documentation build (catches missing docs)
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# Philosophy checks
python3 scripts/philosophy-check.py
```

If any of these fail, fix them before opening a PR. CI is configured to run the exact same checks.

---

## 7. Commit and PR

### Commit message

```
feat(governance-default): add PingPlugin tick counter

Demonstrates minimal Brioche governance plugin pattern.
Counts SystemSignal::Tick events in ExtensionStorage.

Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections
```

### PR template

Fill out every section of `.github/PULL_REQUEST_TEMPLATE.md`. Even for trivial changes. The checklist exists to catch violations before human review.

---

## 8. Common Pitfalls in First PRs

| Mistake | Why it fails | Fix |
|---------|-------------|-----|
| Using `HashMap` in plugin state | Non-deterministic iteration order | Use `BTreeMap` or `IndexMap` |
| Calling `.unwrap()` on an `Option` | Panic in governance is a kernel crash | Use `match` or `if let` |
| Importing `std::time::Instant` | Time must come from the shell, not the plugin | Use `SystemSignal::Tick` |
| Mutating `session.history` directly | Violates mechanism/policy boundary | Return an `Effect::MutateHistory` |
| Adding business logic to `brioche-core` | Core is mechanism only | Move logic to a governance plugin |
| Forgetting doc comments on `pub` items | CI will reject with `RUSTDOCFLAGS=-D warnings` | Every `pub` item gets a doc block |

---

## 9. Where to Go Next

- **Complex plugins:** See existing plugins in `crates/brioche-governance-default/src/` for patterns on trait composition, COW rollback, and epoch handling.
- **New traits:** If your plugin needs a capability that no trait provides, you must define the trait in `brioche-governance` and write an ADR in `docs/adr/`.
- **Property tests:** State-machine plugins require `proptest`. See `tests/prop_*.rs` for examples.

Welcome to Brioche. The compiler is your strictest collaborator.
