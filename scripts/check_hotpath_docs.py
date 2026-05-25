#!/usr/bin/env python3
"""
Scans hot-path modules for pub functions missing complexity/budget docs.
Fails CI if any are found.

Usage:
    python3 scripts/check_hotpath_docs.py
"""

import re
import sys
from pathlib import Path

# List of hot-path modules that must document complexity for every pub fn.
# Expand this list as the codebase grows and functions are documented.
HOT_PATH_MODULES = [
    "crates/brioche-core/src/lib.rs",
    "crates/brioche-core/src/extension.rs",
    "crates/brioche-core/src/types.rs",
    "crates/brioche-governance/src/lib.rs",
]

# Keywords that indicate complexity/budget documentation.
REQUIRED_KEYWORDS = [
    "O(",
    "zero-allocation",
    "no allocation",
    "Complexity",
    "Budget",
    "Hot path",
    "Deterministic",
]


def check_file(path: Path) -> int:
    """Returns the number of violations found in the file."""
    if not path.exists():
        print(f"WARNING: {path} does not exist — skipping")
        return 0

    content = path.read_text()
    violations = 0

    # Find pub fn declarations (including pub(crate), pub unsafe, etc.)
    pub_fn_pattern = re.compile(
        r"^\s*pub(?:\s*\([^)]*\)|\s+unsafe)?\s+fn\s+(\w+)", re.MULTILINE
    )

    for match in pub_fn_pattern.finditer(content):
        fn_name = match.group(1)
        before = content[: match.start()]

        # Extract the last doc comment block preceding the function,
        # skipping any #[...] attributes that may appear between the docs
        # and the function signature.
        lines_before = before.split("\n")
        doc_lines = []
        for line in reversed(lines_before):
            stripped = line.strip()
            if stripped.startswith("///"):
                doc_lines.insert(0, stripped)
            elif stripped == "" or stripped.startswith("#["):
                continue
            else:
                break

        if not doc_lines:
            print(f"VIOLATION: {path}:{fn_name} — missing doc comment")
            violations += 1
            continue

        doc_block = "\n".join(doc_lines)
        if not any(kw in doc_block for kw in REQUIRED_KEYWORDS):
            print(
                f"VIOLATION: {path}:{fn_name} — "
                f"missing complexity/budget note (expected one of: {REQUIRED_KEYWORDS})"
            )
            violations += 1

    return violations


def main() -> int:
    project_root = Path(__file__).parent.parent
    total = 0

    for relative_path in HOT_PATH_MODULES:
        full_path = project_root / relative_path
        total += check_file(full_path)

    if total > 0:
        print(f"\n{total} hot-path documentation violation(s) found.")
        sys.exit(1)

    print("Hot-path documentation OK.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
