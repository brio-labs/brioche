# ADR-002: Settings-Driven Extension Registry Rebuild for Brioche Desktop

## Status

Accepted

## Context

Brioche Desktop 0.1 exposes extension points (memory, tools, skills, settings sections, footer metrics, context engines) through `ExtensionRegistry` in `brioche-shell-persistence`. Until now the registry was built once at startup and shared read-only with the Tauri backend. Two new requirements cross the Shell Persistence / Desktop Apps boundary:

1. **Runtime settings hot-reload**: Changing AMP memory endpoints or model settings in the desktop settings panel must reconfigure the active shell without restarting the application.
2. **Shared async state**: Tauri commands hold `ExtensionRegistry` behind `tokio::sync::RwLock`. Mutating providers such as `SkillRegistry` and `ToolRegistry` previously required `&mut self`, which forced write locks and `Arc::get_mut` gymnastics that were fragile and race-prone.

These needs touch `brioche-shell-persistence` (Book III-A) and `brioche-desktop` (Book IV) at the same time.

## Decision

### 1. Provider state moves behind `std::sync::RwLock`

`SkillRegistry`, `ToolRegistry`, and `AmpMemoryProvider` now store mutable runtime state inside `std::sync::RwLock`. Their trait methods changed from `&mut self` to `&self` where the registry is consumed from shared async state:

- `SkillProvider::set_enabled`, `create_skill`, `delete_skill` are `&self`.
- `ToolProvider::tools`, `user_tools`, `set_enabled`, `add_user_tool`, `remove_user_tool` are `&self`.
- `MemoryProvider::initialize` is `&self`.

`ToolProvider::tools` and `user_tools` now return `Result<..., String>` so tool-listing failures are reported rather than silently ignored.

**Rationale**: The Tauri command layer can keep a read lock on the async `ExtensionRegistry` while individual providers serialize their own mutations. This removes the need for `Arc::get_mut` and avoids holding an async write lock across provider I/O.

### 2. `ExtensionRegistry::default_set_from_settings` rebuilds the registry at runtime

A new constructor `ExtensionRegistry::default_set_from_settings(settings: &Settings)` creates a fresh registry from a `Settings` value. `default_set()` now delegates to it after loading settings from disk. When the user saves settings in the desktop UI, the backend calls this constructor and replaces both `state.extensions` and `factory.extensions`.

**Rationale**: The registry is cheap to rebuild and this keeps provider construction centralized. It makes AMP endpoint changes immediately visible to the next shell build and memory provider initialization.

### 3. Desktop backend rebuilds the active session on settings change

`set_settings` now performs an end-to-end update:

1. Saves the new settings to disk.
2. Rebuilds `ExtensionRegistry` from the new settings.
3. Updates the `ShellFactory` (config, settings, extensions).
4. Rebuilds the active session's shell.
5. Re-initializes memory providers with the current session context.
6. Migrates existing messages into the new session.
7. Persists updated session metadata and notifies the frontend.

**Rationale**: The frontend contract stays unchanged (`setSettings(settings)`). All runtime consequences of a settings change are handled by the backend, keeping UI code policy-free and the kernel untouched.

### 4. `MemoryProvider::initialize` receives session context per session

`DesktopState::initialize_memory_providers` is called whenever a session is created or switched. It builds a `MemorySessionContext` and calls `initialize(ctx)` on every registered memory provider. `AmpMemoryProvider` stores the context in `Arc<RwLock<Option<MemorySessionContext>>>` so the provider remains cloneable and `Send + Sync`.

**Rationale**: Memory providers need session/workspace scoping, but the registry itself is shared. Interior mutability lets immutable provider handles become context-aware on a per-session basis.

### 5. Session metadata persistence is inlined into `SessionManager`

`SessionMetadataStore` is removed. `SessionManager` owns a private `BTreeMap<String, SessionMetadata>`, loads it from `sessions.json` on creation, and persists it on every insert/remove.

**Rationale**: The previous separate metadata store duplicated persistence logic and silently ignored save errors. Inlining it makes metadata lifecycle part of session management and lets callers propagate save failures.

## Consequences

### Positive

- Settings changes take effect without restarting the desktop application.
- Tauri commands no longer need write locks on the extension registry for skill/tool mutations.
- Load failures in memory, tool, and skill providers are logged or propagated instead of silently falling back.
- The cross-book contract is explicit: `brioche-shell-persistence` owns provider traits and construction; `brioche-desktop` owns lifecycle (build, initialize, rebuild).

### Negative

- Public trait signatures changed (`MemoryProvider::initialize`, `ToolProvider` methods, `LocalMemoryProvider::load`, `ToolRegistry::load`). Out-of-tree providers must be updated.
- `std::sync::RwLock` inside providers must never be held across an await point; future maintainers must respect this boundary.
- `AmpMemoryProvider` clones itself to bridge synchronous trait calls into `block_on`. The clone is cheap (shared `Arc` state), but the pattern is a workaround for the sync/async boundary.

### Neutral

- `tracing` is added to `brioche-shell-persistence` for load-failure warnings; it was already a workspace dependency.
- Radix UI primitives, `clsx`, `tailwind-merge`, and `lucide-react` are added to the desktop frontend to support the new settings panel.

## Affected Invariants

| Invariant | Impact |
|-----------|--------|
| I-Shell-Runtime-OnlyIO | **Upheld** — all I/O remains in Shell/Book III-A and App layers; Core is untouched. |
| I-Eco-ExtensionOverMod | **Upheld** — settings-driven behavior is implemented by rebuilding composed extensions, not by modifying mechanism code. |
| I-Eco-OrderedCollections | **Upheld** — provider state continues to use `BTreeMap`/`Vec`; no `HashMap` in persisted extension state. |
| I-Shell-Error-Propagate | **Upheld** — load and save errors are propagated or logged, not swallowed. |

## Book References

- docs/SPECS.md §Book III-A — Shell Runtime extension points and persistence contract.
- docs/SPECS.md §Book III-C — Projection layer (Brioche Desktop) session and shell lifecycle.
- docs/PHILOSOPHY.md §2.1 — Mechanism vs Policy in Code.
- docs/PHILOSOPHY.md §4.4 — Architecture Decision Records in Code.
- CONTRIBUTING.md §Before Submitting PR — Cross-book changes require an ADR.
