# Phase 1 Governance Refactor Specification

## Goal
Adapt the entire workspace to the new generic governance traits introduced in
`crates/kernel/brioche-governance/src/lib.rs` and `crates/kernel/brioche-core/src/plugin.rs`.

## Design
- `brioche-governance` defines generic traits parameterized by `T: BriocheTypes`.
- `brioche-core` defines `CoreTypes` (impl of `BriocheTypes`) and re-exports the
  governance surface.
- All trait object types in `brioche-core` must use `dyn TraitName<CoreTypes>`.
- Plugins implement atomic capability traits (`OnInput<CoreTypes>`,
  `BeforePrediction<CoreTypes>`, etc.) and `BriochePlugin<CoreTypes>` for metadata.
- The `#[brioche_plugin]` macro injects metadata and `as_*` accessors; its
  generated code must reference `::brioche_governance::PluginCapabilities` and
  `::brioche_core::CoreTypes`.

## Concrete tasks

### 1. `crates/kernel/brioche-core` internals
Update every use of governance traits to include `<CoreTypes>`:
- `src/engine/builder.rs`: `Box<dyn BriochePlugin<CoreTypes>>`,
  `Box<dyn EpochInterceptor<CoreTypes>>`, etc.
- `src/engine/router.rs`: `UnifiedRoutingTable` uses `&[Box<dyn BriochePlugin<CoreTypes>>]`;
  `PluginRouter` stores `Vec<Box<dyn BriochePlugin<CoreTypes>>>`.
- `src/engine/hooks.rs`, `src/engine/dispatch.rs`, `src/engine/finalize.rs`,
  `src/engine/transition_support.rs`, `src/engine.rs`: all `dyn Trait` references
  become `dyn Trait<CoreTypes>`, all `plugin.on_input(...)` become
  `plugin.as_on_input().ok_or(...)?.on_input(...)` etc.
- `src/extension.rs`: `Box<dyn CycleRollbackPolicy<CoreTypes>>`.
- `src/types/session.rs`: remove `PluginCapabilities` and `BriochePlugin` docs if any.

### 2. `crates/kernel/brioche-macro`
- `capability_to_tokens` must emit `::brioche_governance::PluginCapabilities::ON_INPUT` etc.
- `brioche_plugin` must accept `impl BriochePlugin<CoreTypes> for T` (preserve generic args).
- It injects `fn name`, `fn capabilities`, `fn priority`, and `as_*` overrides for each
  capability named in the `capabilities` string, returning `Some(self)` cast to the
  appropriate `&dyn Capability<CoreTypes>`.

### 3. `crates/ecosystem/brioche-plugin-kit`
- Update re-exports in `src/lib.rs` and builder signatures in `src/builder.rs`
  to use `dyn Trait<CoreTypes>`.

### 4. `crates/kernel/brioche-governance-default`
Convert every plugin to implement the atomic capability trait that matches its
`capabilities()` mask, plus `BriochePlugin<CoreTypes>` (via macro). Update governance
anchor trait impls (`EpochInterceptor<CoreTypes>`, etc.). Files include:
- `src/guards.rs`
- `src/subroutines.rs`
- `src/tool_pipeline.rs`
- `src/input_guards.rs`
- `src/error_safety.rs`
- `src/negotiation_broker.rs`
- `src/timeouts.rs`
- `src/rollback.rs`
- `src/noop_traits.rs`
- `src/profiles.rs` (build the engine using the new API)
- `src/lib.rs` re-exports

### 5. `crates/ecosystem/brioche-std`
Convert every plugin (`audit_logger.rs`, `circuit_breaker.rs`, `gc_policy.rs`,
`pending_task_manager.rs`, `token_tracker.rs`) to the new atomic-capability pattern.

### 6. Tests and downstream
- `crates/kernel/brioche-core/tests/engine_transition.rs`
- `crates/kernel/brioche-core/tests/property_transition.rs`
- `crates/kernel/brioche-core/benches/benchmarks.rs`
- `crates/ecosystem/brioche-plugin-kit/tests/plugin_macros.rs`
- `crates/ecosystem/brioche-std/tests/standard_plugins.rs`
- `crates/ecosystem/brioche-playground/src/main.rs`
- `crates/ecosystem/brioche-docgen/src/lib.rs`
- `crates/kernel/brioche-plugin-template/src/lib.rs`

### 7. Verification
Run `cargo check --workspace` and fix all errors. Do not run full tests yet.

## Constraints
- No `unwrap()`, `expect()`, `panic!` in `brioche-core`/`brioche-governance` production code.
- Preserve all doc comments and `Refs:` lines.
- Keep the atomic-trait split: each plugin implements exactly one capability trait
  (plus `BriochePlugin<CoreTypes>` for metadata/persistence).
- Do not change behavior; this is a mechanical refactor.
