# Brioche

A deterministic, invariant-governed plugin execution kernel with layered architectural separation.

## Crates

| Crate | Layer | Description |
|-------|-------|-------------|
| `brioche-core` | Core | Synchronous kernel — pure mechanisms |
| `brioche-governance` | Governance | Governance traits — policy contracts |
| `brioche-governance-default` | Governance | Reference governance implementations |
| `brioche-shell-runtime` | Shell | Runtime environment |
| `brioche-shell-persistence` | Shell | Persistence layer |
| `brioche-shell-projection` | Shell | Projection / UI layer |
| `brioche-std` | Ecosystem | Standard plugins |
| `brioche-macro` | — | Procedural macros |
| `brioche-plugin-kit` | — | Developer tooling |
| `brioche-docgen` | — | Documentation generator |
| `brioche-playground` | — | Docker dev environment |

## Quick Start

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).
