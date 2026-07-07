# Changelog

All notable changes to Brioche are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-24

### Added

- Initial release of the Brioche deterministic plugin execution kernel.
- `brioche-core`: synchronous state-machine kernel with `transition()`,
  `ExtensionStorage`, and deterministic `Effect` emission.
- `brioche-governance-default`: injectable governance traits and reference
  implementations (rollback, quarantine, epoch, depth, timeouts, telemetry,
  decision aggregation).
- `brioche-shell-runtime`: async effect executor and engine event loop.
- `brioche-shell-persistence`: Redb-backed session head and delta-message
  persistence with atomic saves and interruptible GC.
- `brioche-shell-projection`: UI widget registry and streaming renderer.
- `brioche-macro`: `BriocheExtensionType` derive and plugin authoring macros.
- `brioche-std`, `brioche-plugin-kit`, `brioche-docgen`, `brioche-playground`:
  standard plugins, SDK, documentation generator, and playground.
- `brioche-provider-openai` / `brioche-tools-system`: OpenAI provider and
  system tool executor for the shell runtime.
- `agent-terminal` and `brioche-desktop`: terminal and Tauri desktop
  applications.
- `cargo-brioche-lint` / `cargo-brioche-lint-invariants`: cargo subcommands
  for plugin and invariant linting.

### Fixed

- `CycleRollbackPolicy` now rolls back extension state when the per-hook COW
  budget is exceeded and emits `PluginFault`.
- `HookEffectConstraint` validation now receives the correct hook index for
  each routed hook.
- The `on_error` plugin hook is invoked for intercepted plugin faults.
- `CowBudgetPolicy` can be injected through `BriocheEngineBuilder`.
- `save_session` writes the session head and message delta in a single Redb
  transaction.
- `session.persisted_msg_count` is advanced from the last successful save
  watermark via the `SessionCallback`.
- `SessionHeadDTO::to_session` restores extension cold snapshots.
- GC uses `compaction_index` rather than `persisted_msg_count` and installs a
  fresh cancellation token after each cancel request.

[Unreleased]: https://github.com/brio-labs/brioche/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/brio-labs/brioche/releases/tag/v0.1.0
