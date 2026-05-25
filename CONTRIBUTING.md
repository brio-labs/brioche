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
- [ ] Commit messages follow Conventional Commits
- [ ] `cargo fmt` and `cargo clippy` pass
- [ ] `cargo deny check all` passes
- [ ] Tests added for Core/Governance changes
- [ ] Documentation updated if behavior changed
- [ ] ADR added for architectural decisions

## Review Process
- All Core/Governance changes require 2 approvals
- Shell changes require 1 approval
- Documentation changes require 1 approval
