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

# 5. Philosophy: unwrap/expect in core mechanism crates
echo "=== Philosophy: unwrap/expect in core ==="
if grep -rq "unwrap()\|expect(" crates/brioche-core/src/ crates/brioche-governance/src/; then
	echo "ERROR: unwrap/expect found in brioche-core or brioche-governance"
	exit 1
fi

# 6. Philosophy: invariant doc format check
# Only enforce Refs: format on item-level doc comments (///), not module docs (!!).
echo "=== Philosophy: invariant references format ==="
VIOLATIONS=$(grep -rn "^\s*///.*I-[A-Z]" --include="*.rs" crates/ |
	grep -v "Refs:" ||
	true)
if [ -n "$VIOLATIONS" ]; then
	echo "ERROR: Item-level doc comments mentioning invariants must use 'Refs: I-...' format. Violations:"
	echo "$VIOLATIONS"
	exit 1
fi

# 8. GPG signing reminder
if ! git verify-commit HEAD 2>/dev/null; then
	echo "WARNING: Last commit is not GPG signed. Configure signing."
fi

echo "Pre-commit checks passed."
