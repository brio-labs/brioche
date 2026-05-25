# Contributing to Brioche

## Prerequisites
- [ ] Rust 1.95+ installed
- [ ] GPG key configured and added to GitHub
- [ ] `cargo-deny`, `cargo-nextest` installed

## Setup
1. Fork and clone
2. Run `./scripts/setup-gpg.sh` (verifies GPG config)
3. Run `cargo test --workspace` to verify environment
4. Install pre-commit hook: `cp scripts/pre-commit.sh .git/hooks/pre-commit`

## Before Submitting PR
- [ ] Commits are GPG-signed (`git log --show-signature`)
- [ ] Commit messages follow the [project-specific format](#commit-message-format) below
- [ ] `cargo fmt` and `cargo clippy` pass
- [ ] `cargo deny check all` passes
- [ ] Tests added for Core/Governance changes
- [ ] Documentation updated if behavior changed
- [ ] ADR added for architectural decisions

## Commit Message Format

All commits **must** use the following format:

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

The book (subsystem / crate) the change affects. Examples:
- `core`, `governance`, `shell`, `storage`, `consensus`

Use lowercase and kebab-case for multi-word book names.

### `<description>` — Required

A concise, imperative-mood summary (no trailing period).  
Max 72 characters.

### Body — Optional

Explain **what** changed and **why**, not just how. Wrap at 72 characters.

### Footer(s) — Optional

- `Closes: #<issue>` / `Fixes: #<issue>`
- `Refs: <adr-id>, <adr-id>`
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

## Review Process
- All Core/Governance changes require 2 approvals
- Shell changes require 1 approval
- Documentation changes require 1 approval
