## Summary
<!-- What does this PR change? -->

## Affected Layers
- [ ] Core
- [ ] Governance
- [ ] Shell Runtime
- [ ] Shell Persistence
- [ ] Shell Projection
- [ ] Ecosystem / Tooling

## Type of Change
- [ ] Mechanism change (modifies kernel behavior)
- [ ] Policy change (new/modified plugin or trait)
- [ ] Invariant addition/modification
- [ ] Documentation only
- [ ] Performance optimization
- [ ] Breaking change

## Verification
- [ ] `cargo test` passes
- [ ] `cargo deny check all` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- [ ] Property tests added/updated (`proptest`)
- [ ] Replay tests pass (if Core / Governance modified)
- [ ] Benchmarks show no regression (`criterion`)
- [ ] Documentation updated (if behavior changed)

## Philosophy Checklist (Bot-enforced + Human-verified)
- [ ] No `HashMap`/`HashSet` in persisted state (unless explicitly exempted with `#[allow(clippy::disallowed_types)]` + comment)
- [ ] All `pub` items have doc comments with `Refs: I-...` where applicable
- [ ] Hot path functions document complexity/budget
- [ ] No `unwrap`/`expect` in `brioche-core` or `brioche-governance`
- [ ] No business logic in mechanism code (Core)
- [ ] Trait implementations are atomic (no inheritance-like coupling)
- [ ] ADR linked if crossing book boundaries

## Reviewer Checklist
1. Does this uphold or violate any invariant? If behavior changes, spec is updated.
2. Is this mechanism or policy? If policy, it must be in a plugin/trait.
3. Where is the `proptest`? New state machines require property tests.
4. Is the documentation lying? Check that docs match code behavior.

## Invariant Impact
<!-- List any invariants or behavioral constraints affected -->
-
