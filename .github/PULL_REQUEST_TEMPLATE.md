## Summary
<!-- What changed and why? -->

## Invariant Impact
<!-- List affected invariants. Update docs/SPECS.md if behavior changes. -->
-

## Brioche Philosophy Checklist
- [ ] No `HashMap`/`HashSet` in persisted state (unless explicitly exempted)
- [ ] All `pub` items have doc comments with `Refs: I-...`
- [ ] Hot path functions document complexity/budget
- [ ] No `unwrap`/`expect` in `brioche-core`
- [ ] No business logic in mechanism code (Core)
- [ ] Trait implementations are atomic (no inheritance-like coupling)
- [ ] ADR linked if crossing book boundaries
- [ ] No one-file-per-type fragmentation: related plugins share a module
- [ ] No trivial `*State` structs that only mirror plugin config
- [ ] No redundant `O(1)` / `Never panics` docs on obvious accessors
- [ ] No stale backup/generated artifacts in production source or workflow paths
- [ ] Oversized source/test/tool files are split or carry architectural exemptions

## Human Checks (not enforced by CI)
- [ ] Does this change uphold or violate any invariant? If it changes behavior, the spec is updated.
- [ ] Is this mechanism or policy? Policy stays in plugins/traits, never Core.
- [ ] New state machines have `proptest` coverage.
- [ ] Cross-book changes link an ADR.
- [ ] Documentation matches code behavior.
- [ ] Related modules remain cohesive; small one-type files are not introduced.
- [ ] New `*State` types carry mutable runtime state, not copied config.
- [ ] Docs add invariant signal, not boilerplate ceremony.
- [ ] Large test suites are grouped by invariant/contract, not chronology.
