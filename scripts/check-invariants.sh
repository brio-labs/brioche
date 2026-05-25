#!/bin/bash
set -e

echo "Running invariant checks..."

# 1. Verify no unlicensed dependencies
cargo deny check licenses

# 2. Verify no known vulnerabilities
cargo deny check advisories

# 3. Verify no banned crate patterns
cargo deny check bans

# 4. Verify source policy (only crates.io + brio-labs org)
cargo deny check sources

echo "All invariant checks passed."
