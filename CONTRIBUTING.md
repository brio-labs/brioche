# Contributing to Brioche

> **⚠️ STOP.** Before writing code, read [`docs/PHILOSOPHY.md`](docs/PHILOSOPHY.md).
> Brioche is not a typical Rust project. It rejects OOP inheritance, runtime polymorphism,
> and hidden state. PRs that violate the philosophy are rejected regardless of test coverage.
> The compiler and CI are configured to enforce this — no exceptions.
>
> New to Brioche? Follow [`docs/first-pr-guide.md`](docs/first-pr-guide.md) for a step-by-step walkthrough.

## Prerequisites

- Rust **1.95+**
- GPG key configured for Git commit signing
- `cargo-deny`, `cargo-nextest`, `cargo-brioche-lint-invariants`
- `brioche-docgen` (for documentation changes)

```bash
rustup toolchain install stable
rustup component add rustfmt clippy
cargo install cargo-deny cargo-nextest
cargo install --path crates/infra/cargo-brioche-lint-invariants
cargo install --path crates/ecosystem/brioche-docgen
```

## Repository Setup

```bash
git clone git@github.com:brio-labs/brioche.git
cd brioche

# Verify GPG signing is active
./scripts/setup-gpg.sh

# Install the pre-commit hook
cp scripts/pre-commit.sh .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit

# Verify the workspace compiles and tests pass
cargo test --workspace
```

## Development Workflow

1. **Create a branch** from `main`:
   - `feature/<book>-<description>` for new functionality
   - `fix/<book>-<description>` for bug fixes
   - `doc/<description>` for documentation-only changes
   - `chore/<description>` for tooling or CI changes

2. **Update the specification** in `/docs/architecture/` before writing code if your change affects behavior, trait interfaces, or invariants.

3. **Write code and tests.** See [Code Standards](#code-standards) and [Testing Requirements](#testing-requirements).

4. **Open a Pull Request.** Fill out the PR template completely.

## Code Standards

### Formatting and Linting

All code must pass:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check all
cargo brioche-lint-invariants check-refs --root crates/
cargo brioche-lint-invariants check-matrix
```

### Determinism and Collections

- **Persisted state** (types deriving `BriocheExtensionType`) must use ordered collections: `BTreeMap`, `BTreeSet`, `IndexMap`.
- **`HashMap`/`HashSet`** are permitted only in transient, non-persisted caches internal to a plugin. Add a comment explaining why it is safe.
- `Vec` fields in persisted state must be annotated with `#[brioche(deterministic_order)]` unless exempt by semantics (stack, FIFO buffer).

### Error Handling

- `unwrap()`, `expect()`, and `panic!()` are **denied by clippy** in `brioche-core` and `brioche-governance`.
- Use `Result<T, BriocheError>` for system errors and `PluginResult<T>` for plugin hooks.
- Never silently ignore errors. Log or propagate.

### Architecture Boundaries

| Rule | Violation Example |
|------|-------------------|
| `brioche-core` contains mechanism only. No business rules. | Adding a quarantine check inside `transition()` dispatch. |
| Governance logic lives in traits and plugins. | Hard-coding an epoch check in `BriocheEngine` instead of `EpochInterceptor`. |
| Plugins never mutate `Session` fields directly. | Modifying `session.history` instead of returning `MutateHistory`. |
| Plugins never access `session.state` directly. | Reading `session.state` instead of using `SessionSnapshot` from `ExtensionStorage`. |
| Effects are the only side-effect channel. | Performing I/O inside a plugin hook. |

### Documentation

Every `pub` item must have a doc comment containing:

1. **What it does** (one sentence).
2. **Invariant references** if applicable: `Refs: I-Category-Name`.
3. **Complexity contract** for hot-path functions (e.g., `O(1)`, zero-allocation).
4. **Error conditions** for functions returning `Result`.

```rust
/// Validates effect permission via pre-computed bitmask.
///
/// Refs: I-Core-HookEffect-O1
///
/// Complexity: O(1). No heap allocation.
fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;
```

Module roots (`lib.rs`, `mod.rs`) must have a `//!` block describing the module's Book affiliation and the invariants it upholds.

### Comments

Explain **why**, not what. The code shows what happens; comments justify deviations from the obvious or reference architectural constraints.

```rust
// Increment epoch to orphan in-flight async tasks.
// Satisfies I-Gov-Epoch-Reject.
epoch_state.current_generation += 1;
```

## Common First-PR Mistakes

New Rust developers will instinctively reach for `HashMap`, `unwrap()`, and inheritance. Use this Rosetta stone:

| "I want to..." | ❌ Don't | ✅ Do instead |
|----------------|---------|---------------|
| Cache lookups in a plugin | `HashMap<K, V>` | `BTreeMap<K, V>` or `IndexMap` |
| Handle a missing value | `.unwrap()` or `.expect()` | `match`, `if let`, or `?` with explicit error |
| Share behavior between plugins | `struct BasePlugin` + inheritance | Trait composition (`impl TraitA + TraitB`) |
| Get current time in a plugin | `Instant::now()` | Receive `SystemSignal::Tick` from the shell |
| Branch on a business rule in Core | `if user_is_banned` inside `transition()` | Return an effect; let a governance plugin decide |
| Clone all extensions before a hook | `ext.clone()` (O(n) extensions) | COW via `UndoFrameState` (O(k) mutated) |
| Skip doc comments on `pub` items | Leave it bare | Every `pub` item gets a doc block with `Refs:` |

## Testing Requirements

### Test Types by Change

| Change Location | Required Tests |
|-----------------|---------------|
| `brioche-core` | Property tests (`proptest`), replay tests |
| `brioche-governance` traits | Unit tests + scenario tests |
| Governance plugins | Unit tests with mocked `ExtensionStorage` |
| Hot path changes | Criterion benchmarks (see below) |
| `brioche-macro` | `trybuild` compilation tests (positive and negative) |
| Shell layers | Async integration tests |

### Property Tests

State-machine changes in Core require `proptest` coverage. Tests must verify:

- `transition()` never panics for arbitrary valid inputs.
- Identical inputs produce identical `Vec<<Effect>` outputs (bit-for-bit determinism).
- Invalid state transitions produce `BriocheError`, not panics.

### Replay Tests

If you modify `transition()`, `EngineInput` handling, or `Effect` emission:

1. Record an `AuditState` log.
2. Replay it in a fresh engine.
3. Assert the effect sequence and final state match exactly.

### Benchmarks

Hot path modifications require Criterion benchmarks:

| Suite | Threshold |
|-------|-----------|
| `stream_latency` | P99 < 50 µs |
| `extension_lookup` | < 50 ns |
| `hook_effect_o1` | < 100 ns |
| `cow_rollback` | < 10 µs |

Run benchmarks before and after your change:

```bash
cargo bench --profile headless -- <suite_name>
```

A regression > 150% blocks merge.

## Commit Message Format

```
<type>(<book>): <description>

[optional body]

[optional footer(s)]
```

### `<type>` — Required

| Type | Description |
|------|-------------|
| `feat` | New trait, plugin, or mechanism |
| `fix` | Bug fix |
| `docs` | Documentation (specs, ADRs, inline docs) |
| `test` | Tests only (property, replay, benchmark) |
| `refactor` | Code change without behavior change |
| `perf` | Performance improvement (hot path, allocation reduction) |
| `chore` | CI, tooling, dependencies |
| `invariant` | New or modified system invariant |

### `<book>` — Required

The subsystem the change affects:

`core`, `governance`, `governance-default`, `shell-runtime`, `shell-persistence`, `shell-projection`, `std`, `macro`, `plugin-kit`, `playground`, `docgen`, `repo`

### `<description>` — Required

Imperative mood, max 72 characters, no trailing period.

### Body — Optional

Explain what changed and why. Wrap at 72 characters.

### Footer(s) — Optional

- `Closes: #<issue>`
- `Refs: <invariant-id>, <invariant-id>` (e.g., `Refs: I-Core-Pure, I-Gov-Epoch-Reject`)
- `Co-authored-by: Name <email>`

### Examples

```
fix(governance): prevent duplicate SubRoutine cleanup

SubRoutineCleanupGuard now checks exit_counts before removal.
Adds defensive orphan cleanup.

Closes: #42
```

```
feat(core): implement granular COW snapshot in ExtensionStorage

Implements CycleRollbackPolicy trait with VTable clone_box.
Adds UndoFrameGuard as reference implementation.

Refs: I-Gov-Rollback-BestEffort, I-Core-VTableClone
```

## Before Submitting PR

- [ ] Branch is up to date with `main` (rebased, not merged).
- [ ] All commits are GPG-signed (`git log --show-signature`).
- [ ] Commit messages follow the format above.
- [ ] `cargo brioche-lint-invariants check-refs --root crates/` and `cargo brioche-lint-invariants check-matrix` pass.
- [ ] Tests added for Core/Governance changes.
- [ ] Replay tests pass if `transition()` or effect emission changed.
- [ ] Benchmarks show no regression if hot path modified.
- [ ] Documentation updated (`/docs/architecture/` and inline docs).
- [ ] ADR added if the change introduces a new trait, modifies `GovernanceCompatibilityMatrix`, or crosses book boundaries.
- [ ] `cargo doc --workspace --no-deps` builds without warnings.

## Review Process

| Change Type | Required Approvals |
|-------------|------------------|
| Core (`brioche-core`, `brioche-macro`) | 2 (System Architect + Core Engineer) |
| Governance (`brioche-governance`, `brioche-governance-default`) | 2 (System Architect + Core Engineer) |
| Shell (`brioche-shell-*`) | 1 (Shell Engineer) |
| Ecosystem / Tooling (`brioche-std`, `plugin-kit`, `docgen`, `playground`) | 1 (Tooling Engineer) |
| Documentation only | 1 (any maintainer) |

**Cross-book changes** require an ADR and approval from the System Architect.

Reviewers verify:

1. The change respects the mechanism/policy boundary.
2. Invariants are upheld, not violated.
3. Tests cover the change adequately.
4. Documentation matches the implementation.

**PS: THIS REVIEW PROCESS IS NOT IN EFFECT AS FOR THE MOMENT THE TEAM IS ONE PERSON. WILL BE APPLICABLE LATER IF THE TEAM GROWS**
## Release Tags

All release tags must be GPG-signed:

```bash
git tag -s v0.1.0 -m "Release v0.1.0"
git verify-tag v0.1.0
git push origin v0.1.0
```

## Questions?

Open a Discussion for development questions. Use Issues for bug reports and feature requests only.
