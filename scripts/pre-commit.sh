#!/bin/bash
set -e

echo "Running pre-commit checks..."

# 1. Format check
cargo fmt -- --check

# 2. Clippy (strict)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 3. Deny (licenses, advisories, bans)
cargo deny check all

# 4. Fast unit tests (not property tests — too slow for pre-commit)
cargo test --workspace --lib -- --test-threads=4

# 5. GPG signing reminder
if ! git verify-commit HEAD 2>/dev/null; then
	echo "WARNING: Last commit is not GPG signed. Configure signing."
fi

echo "Pre-commit checks passed."
