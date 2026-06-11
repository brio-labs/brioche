#!/bin/bash
set -e

echo "Running pre-commit checks..."

# 1. Format check (stable)
cargo fmt -- --check

# 2. Clippy (strict)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 3. Deny (licenses, advisories, bans, sources)
cargo deny check all

# 4. Fast unit tests (not property tests — too slow for pre-commit)
cargo test --workspace --lib -- --test-threads=4

# 5. Custom Brioche lint crates — mechanism vs policy, invariant refs
echo "=== cargo-brioche-lint ==="
# cargo-brioche-lint is designed for plugin crates (policy layer).
# brioche-core is mechanism — direct Session access there is legitimate.
cargo run --package cargo-brioche-lint -- --path crates/brioche-governance
cargo run --package cargo-brioche-lint -- --path crates/brioche-plugin-template

echo "=== cargo-brioche-lint-invariants ==="
cargo run --package cargo-brioche-lint-invariants -- check-refs --root crates/

# 6. Philosophy: unwrap/expect in core mechanism crates
echo "=== Philosophy: unwrap/expect in core ==="
if grep -rq "unwrap()\|expect(" crates/brioche-core/src/ crates/brioche-governance/src/; then
	echo "ERROR: unwrap/expect found in brioche-core or brioche-governance"
	exit 1
fi

# 7. Philosophy: invariant doc format check
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

# 8. Module-level docs (!!) for every crate lib.rs
echo "=== Philosophy: module-level docs ==="
for lib in crates/*/src/lib.rs; do
	if ! head -20 "$lib" | grep -q "^//!"; then
		echo "ERROR: missing //! module doc in $lib"
		exit 1
	fi
done

# 9. Session !Send/!Sync marker
echo "=== Philosophy: Session !Send/!Sync marker ==="
if ! grep -q "_not_send_sync\|PhantomData<\*mut ()>" crates/kernel/brioche-core/src/types/session.rs; then
	echo "ERROR: Session struct missing !Send/!Sync marker"
	exit 1
fi

# 10. GPG signing reminder
if ! git verify-commit HEAD 2>/dev/null; then
	echo "WARNING: Last commit is not GPG signed. Configure signing."
fi

echo "Pre-commit checks passed."
