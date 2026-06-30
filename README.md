# Brioche

A deterministic, invariant-governed plugin execution kernel with layered architectural separation.

## Crates

| Crate | Layer | Description |
|-------|-------|-------------|
| `agent-terminal` | Apps | Minimal terminal agent |
| `brioche-core` | Kernel | Synchronous kernel — pure mechanisms |
| `brioche-desktop` | Apps | Tauri desktop GUI |
| `brioche-docgen` | Ecosystem | Documentation generator |
| `brioche-governance-default` | Kernel | Reference governance implementations |
| `brioche-macro` | Kernel | Procedural macros |
| `brioche-playground` | Ecosystem | Docker dev environment and playground |
| `brioche-plugin-kit` | Ecosystem | Developer tooling and SDK |
| `brioche-provider-openai` | Providers | OpenAI-compatible LLM provider |
| `brioche-reedline` | Infra | Shared terminal infrastructure |
| `brioche-shell-persistence` | Shell | Persistence layer |
| `brioche-shell-projection` | Shell | Projection / UI layer |
| `brioche-shell-runtime` | Shell | Runtime environment |
| `brioche-std` | Ecosystem | Standard plugins |
| `brioche-tools-system` | Tools | System tool executor |
| `cargo-brioche-lint` | Infra | Cargo subcommand for linting plugins |
| `cargo-brioche-lint-invariants` | Infra | Cargo subcommand for invariant checks |

## Quick Start

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).
