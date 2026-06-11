#!/usr/bin/env python3
"""
Comprehensive philosophy compliance checker for Brioche.

Enforces the rules from docs/PHILOSOPHY.md that are not already covered
by clippy, rustfmt, or RUSTDOCFLAGS.

Exit code 0 = all checks passed.
Exit code 1 = one or more philosophy violations found.
"""

import re
import sys
from dataclasses import dataclass
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent


@dataclass
class Violation:
    check: str
    file: Path
    line: int
    message: str


class CheckResult:
    def __init__(self, name: str):
        self.name = name
        self.violations: list[Violation] = []

    def add(self, file: Path, line: int, message: str):
        self.violations.append(Violation(self.name, file, line, message))

    def ok(self) -> bool:
        return len(self.violations) == 0

    def report(self) -> int:
        if self.ok():
            print(f"  ✓ {self.name}")
            return 0
        print(f"  ✗ {self.name} — {len(self.violations)} violation(s)")
        for v in self.violations:
            rel = v.file.relative_to(PROJECT_ROOT)
            print(f"      {rel}:{v.line}  {v.message}")
        return len(self.violations)


# ---------------------------------------------------------------------------
# 1. Hot-path documentation
# ---------------------------------------------------------------------------

HOT_PATH_MODULES = [
    "crates/kernel/brioche-core/src/lib.rs",
    "crates/kernel/brioche-core/src/engine.rs",
    "crates/kernel/brioche-core/src/engine/dispatch.rs",
    "crates/kernel/brioche-core/src/engine/finalize.rs",
    "crates/kernel/brioche-core/src/engine/helpers.rs",
    "crates/kernel/brioche-core/src/engine/hooks.rs",
    "crates/kernel/brioche-core/src/engine/router.rs",
    "crates/kernel/brioche-core/src/engine/trace.rs",
    "crates/kernel/brioche-core/src/engine/types.rs",
    "crates/kernel/brioche-core/src/engine/builder.rs",
    "crates/kernel/brioche-core/src/extension.rs",
    "crates/kernel/brioche-core/src/types.rs",
    "crates/kernel/brioche-core/src/plugin.rs",
    "crates/kernel/brioche-governance/src/lib.rs",
]

COMPLEXITY_KEYWORDS = [
    "O(",
    "zero-allocation",
    "no allocation",
    "Complexity",
    "Budget",
    "Hot path",
    "Deterministic",
    "O(log",
    "O(1)",
    "O(n)",
]


def _collect_doc_block(lines: list[str], decl_line: int) -> list[str]:
    """Walk backwards from the line BEFORE the declaration to collect doc comments."""
    doc_lines: list[str] = []
    for idx in range(decl_line - 2, -1, -1):
        stripped = lines[idx].strip()
        if stripped.startswith("///"):
            doc_lines.insert(0, stripped)
        elif stripped == "" or stripped.startswith("#"):
            continue
        else:
            break
    return doc_lines


def check_hotpath_docs() -> CheckResult:
    result = CheckResult("Hot-path documentation")
    pub_item_re = re.compile(
        r"^\s*pub(?:\s*\([^)]*\)|\s+unsafe)?(?:\s*\(\s*crate\s*\))?\s+(fn|struct|enum|trait)\s+(\w+)",
        re.MULTILINE,
    )

    for rel in HOT_PATH_MODULES:
        path = PROJECT_ROOT / rel
        if not path.exists():
            result.add(path, 0, "file does not exist — skipping")
            continue

        content = path.read_text()
        lines = content.split("\n")

        for m in pub_item_re.finditer(content):
            item_type = m.group(1)
            item_name = m.group(2)
            pos = m.start()
            line_no = content[:pos].count("\n") + 1

            doc_lines = _collect_doc_block(lines, line_no)

            # Only flag items that *have* docs but lack complexity notes.
            # Missing docs is covered by `cargo doc -D warnings`.
            if not doc_lines:
                continue

            doc_block = "\n".join(doc_lines)
            if not any(kw in doc_block for kw in COMPLEXITY_KEYWORDS):
                result.add(
                    path,
                    line_no,
                    f"pub {item_type} `{item_name}` missing complexity/budget note",
                )

    return result


# ---------------------------------------------------------------------------
# 1b. Panic / safety / error contracts on hot-path pub items
# ---------------------------------------------------------------------------

PANIC_SAFETY_KEYWORDS = [
    "# Panics",
    "# Safety",
    "# Errors",
    "Never panics",
    "never panics",
    "no panic",
    "cannot panic",
]


def check_panic_safety_docs() -> CheckResult:
    result = CheckResult("Panic/safety contracts")
    pub_item_re = re.compile(
        r"^\s*pub(?:\s*\([^)]*\)|\s+unsafe)?(?:\s*\(\s*crate\s*\))?\s+(fn|struct|enum|trait)\s+(\w+)",
        re.MULTILINE,
    )

    for rel in HOT_PATH_MODULES:
        path = PROJECT_ROOT / rel
        if not path.exists():
            continue

        content = path.read_text()
        lines = content.split("\n")

        for m in pub_item_re.finditer(content):
            item_type = m.group(1)
            item_name = m.group(2)
            pos = m.start()
            line_no = content[:pos].count("\n") + 1

            doc_lines = _collect_doc_block(lines, line_no)
            if not doc_lines:
                continue

            doc_block = "\n".join(doc_lines)
            if not any(kw in doc_block for kw in PANIC_SAFETY_KEYWORDS):
                result.add(
                    path,
                    line_no,
                    f"pub {item_type} `{item_name}` missing panic/safety/error contract",
                )

    return result


# ---------------------------------------------------------------------------
# 2. Invariant references on pub items in Core & Governance
# ---------------------------------------------------------------------------

INVARIANT_CRATES = [
    # Book I — Core
    "crates/kernel/brioche-core/src",
    "crates/kernel/brioche-macro/src",
    # Book II — Governance
    "crates/kernel/brioche-governance/src",
    "crates/kernel/brioche-governance-default/src",
    # Book III-A — Shell Runtime
    "crates/runtime/brioche-shell-runtime/src",
    "crates/runtime/brioche-shell-persistence/src",
    "crates/runtime/brioche-shell-projection/src",
    # Book III-B — Providers
    "crates/providers/brioche-provider-openai/src",
    # Book III-C — Tools
    "crates/tools/brioche-tools-system/src",
    # Book IV — Apps
    "crates/apps/agent-terminal/src",
    # Infrastructure
    "crates/infra/brioche-reedline/src",
    "crates/infra/cargo-brioche-lint/src",
    "crates/infra/cargo-brioche-lint-invariants/src",
    "crates/ecosystem/brioche-docgen/src",
    "crates/ecosystem/brioche-playground/src",
    "crates/ecosystem/brioche-plugin-kit/src",
    "crates/ecosystem/brioche-std/src",
]

INVARIANT_PATTERNS = [
    "Refs: I-",
    "Refs: SPECS",
    "# Invariants",
    "# Invariant",
]


def check_invariant_refs() -> CheckResult:
    result = CheckResult("Invariant references")
    pub_item_re = re.compile(
        r"^\s*pub(?:\s*\([^)]*\)|\s+unsafe)?(?:\s*\(\s*crate\s*\))?\s+(fn|struct|enum|trait)\s+(\w+)",
        re.MULTILINE,
    )

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            content = path.read_text()
            lines = content.split("\n")

            for m in pub_item_re.finditer(content):
                item_type = m.group(1)
                item_name = m.group(2)
                pos = m.start()
                line_no = content[:pos].count("\n") + 1

                doc_lines = _collect_doc_block(lines, line_no)

                # Only check items that already have docs.
                # Missing docs on `pub` items is enforced by `cargo doc -D warnings`.
                # We only check items that already have docs but lack invariant refs.
                if not doc_lines:
                    continue

                doc_block = "\n".join(doc_lines)
                if not any(pat in doc_block for pat in INVARIANT_PATTERNS):
                    result.add(
                        path,
                        line_no,
                        f"pub {item_type} `{item_name}` doc missing invariant ref "
                        f"(expected 'Refs: I-...' or '/// # Invariants')",
                    )

    return result


# ---------------------------------------------------------------------------
# 3. BriocheExtensionType structs document snapshot strategy
# ---------------------------------------------------------------------------

SNAPSHOT_KEYWORDS = [
    "snapshot",
    "COW",
    "clone",
    "weight",
    "bytes",
    "memory",
    "footprint",
    "size",
    "budget",
]


def check_extension_type_docs() -> CheckResult:
    result = CheckResult("Extension-type snapshot docs")

    for path in PROJECT_ROOT.rglob("*.rs"):
        # Skip tests, benches, and macro UI tests — they don't need production docs.
        if "tests" in path.parts or "benches" in path.parts:
            continue
        if path.name.startswith("fail_") or path.name.startswith("pass_"):
            continue

        content = path.read_text()
        lines = content.split("\n")

        # Find lines mentioning BriocheExtensionType that are not comments or imports.
        for derive_line, line in enumerate(lines):
            if "BriocheExtensionType" not in line:
                continue
            stripped = line.strip()
            if stripped.startswith(("//", "use ")):
                continue

            # Verify this line belongs to a #[derive(...) block.
            is_derive = False
            for k in range(max(0, derive_line - 20), derive_line + 1):
                if lines[k].strip().startswith("#[derive"):
                    is_derive = True
                    break
            if not is_derive:
                continue

            # Find the start of the derive block (for doc collection).
            derive_start = derive_line
            for k in range(derive_line, -1, -1):
                if lines[k].strip().startswith("#[derive"):
                    derive_start = k
                    break

            # Walk forward to find the associated pub struct.
            struct_idx = None
            for j in range(derive_line + 1, len(lines)):
                stripped = lines[j].strip()
                if stripped.startswith("pub struct "):
                    struct_idx = j
                    break
                if stripped == "" or stripped.startswith(("#", "//")):
                    continue
                # Allow closing paren of a multi-line derive block.
                if stripped.startswith(")"):
                    continue
                break

            if struct_idx is None:
                continue

            # Collect doc block above the derive block.
            doc_lines: list[str] = []
            for idx in range(derive_start - 1, -1, -1):
                stripped = lines[idx].strip()
                if stripped.startswith("///"):
                    doc_lines.insert(0, stripped)
                elif stripped == "":
                    continue
                else:
                    break

            if not doc_lines:
                struct_name = lines[struct_idx].strip().split()[2]
                result.add(
                    path,
                    struct_idx + 1,
                    f"`{struct_name}` (BriocheExtensionType) missing doc comment",
                )
                continue

            doc_block = "\n".join(doc_lines)
            if not any(kw.lower() in doc_block.lower() for kw in SNAPSHOT_KEYWORDS):
                struct_name = lines[struct_idx].strip().split()[2]
                result.add(
                    path,
                    struct_idx + 1,
                    f"`{struct_name}` (BriocheExtensionType) missing snapshot/COW docs",
                )

    return result


# ---------------------------------------------------------------------------
# 4. Determinism guards — forbidden patterns in Core & Governance
# ---------------------------------------------------------------------------

DETERMINISM_FORBIDDEN = [
    (re.compile(r"\brand::\b|\buse rand\b|\bextern crate rand\b"), "rand usage"),
    (re.compile(r"\basync\s+fn\b|\basync_trait\b"), "async fn / async_trait"),
    (re.compile(r"\bstd::time::Instant::now\b"), "Instant::now"),
    (re.compile(r"\bstd::thread::sleep\b|\bthread::sleep\b"), "thread::sleep"),
    (
        re.compile(r"\bstd::sync::Mutex\b|\bsync::Mutex\b"),
        "std::sync::Mutex (use parking_lot or atomic)",
    ),
]

DETERMINISM_CRATES = [
    "crates/kernel/brioche-core/src",
    "crates/kernel/brioche-governance/src",
    "crates/kernel/brioche-governance-default/src",
]


def check_determinism() -> CheckResult:
    result = CheckResult("Determinism guards")

    for rel in DETERMINISM_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            # Skip tests — determinism rules apply to production code
            if "tests" in path.parts or path.name.endswith("_test.rs"):
                continue

            content = path.read_text()
            lines = content.split("\n")

            for pat, desc in DETERMINISM_FORBIDDEN:
                for m in pat.finditer(content):
                    line_no = content[: m.start()].count("\n") + 1
                    # Skip lines that are comments
                    if lines[line_no - 1].strip().startswith("//"):
                        continue
                    result.add(path, line_no, f"forbidden: {desc}")

    return result


# ---------------------------------------------------------------------------
# 4b. Vtable / dyn trait usage in Core transition hot path
#     PHILOSOPHY.md §1: "Polymorphic dispatch (vtables) — Runtime indirection
#     in the hot path. Cache-unfriendly."
#
# Scope is intentionally narrow: we only flag vtables inside the actual
# `transition()` hot path (`engine.rs`, `engine/types.rs`, `engine/hooks.rs`).
# Vtables in `extension.rs` are required for type-erased `ExtensionStorage`
# and are documented there as a deliberate design trade-off. Vtables in
# `plugin.rs` and `engine/builder.rs` / `engine/router.rs` are build-time
# containers, not hot-path traversal.
# ---------------------------------------------------------------------------

VTABLE_HOT_PATH_MODULES = [
    "crates/kernel/brioche-core/src/engine.rs",
    "crates/kernel/brioche-core/src/engine/types.rs",
    "crates/kernel/brioche-core/src/engine/hooks.rs",
]


def check_vtable_in_core() -> CheckResult:
    result = CheckResult("Vtable usage in Core hot path")
    vtable_patterns = [
        (re.compile(r"Box\s*<\s*dyn\b"), "Box<dyn Trait>"),
        (re.compile(r"&\s*dyn\b"), "&dyn Trait"),
    ]

    for rel in VTABLE_HOT_PATH_MODULES:
        path = PROJECT_ROOT / rel
        if not path.exists():
            continue

        content = path.read_text()
        lines = content.split("\n")

        for pat, desc in vtable_patterns:
            for m in pat.finditer(content):
                line_no = content[: m.start()].count("\n") + 1
                line = lines[line_no - 1].strip()
                if line.startswith("//"):
                    continue

                # Allow lines that reference the documented architectural
                # exception (e.g., GovernanceKernel's ## Architectural Note).
                # We use a generous window so that a single struct-level note
                # can cover all of its vtable-carrying fields.
                surrounding = "\n".join(
                    lines[max(0, line_no - 30) : min(len(lines), line_no + 5)]
                )
                if "Architectural Note" in surrounding or "ADR:" in surrounding:
                    continue

                result.add(
                    path,
                    line_no,
                    f"vtable indirection: {desc} — "
                    f"PHILOSOPHY.md §1 recommends pre-routing tables; "
                    f"document the exception with an ## Architectural Note or ADR: reference",
                )

    return result


# ---------------------------------------------------------------------------
# 5. Panic guards in Core (backup to clippy workspace lints)
# ---------------------------------------------------------------------------

PANIC_PATTERNS = [
    (re.compile(r"\.unwrap\(\)"), "unwrap()"),
    (re.compile(r"\.unwrap_or\("), "unwrap_or(...)"),
    (re.compile(r"\.unwrap_or_else\("), "unwrap_or_else(...)"),
    (re.compile(r"\.unwrap_or_default\("), "unwrap_or_default()"),
    (re.compile(r"\.expect\("), "expect(...)"),
    (re.compile(r"\bpanic!\("), "panic!(...)"),
    (
        re.compile(r"(?<!\w)\w+\[[^\]]+\]"),
        "direct indexing (potential panic path)",
    ),
]


def check_panic_guards() -> CheckResult:
    result = CheckResult("Panic guards (Core)")
    core_src = PROJECT_ROOT / "crates" / "kernel" / "brioche-core" / "src"

    for path in core_src.rglob("*.rs"):
        if "tests" in path.parts:
            continue

        content = path.read_text()
        lines = content.split("\n")

        for i, line in enumerate(lines):
            if line.strip().startswith("#[cfg(test)]"):
                break
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            for pat, desc in PANIC_PATTERNS:
                if pat.search(line):
                    result.add(path, i + 1, f"forbidden: {desc}")

    return result


# ---------------------------------------------------------------------------
# 6. Trait hierarchies — no supertrait taxonomies
# ---------------------------------------------------------------------------

ALLOWED_SUPERTRAITS = {
    "Send",
    "Sync",
    "Clone",
    "Copy",
    "Debug",
    "Default",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Serialize",
    "Deserialize",
    "SerializeOwned",
    "DeserializeOwned",
    "Sized",
    "Fn",
    "FnOnce",
    "FnMut",
    "Iterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "FusedIterator",
    "From",
    "Into",
    "AsRef",
    "AsMut",
    "Deref",
    "DerefMut",
    "Drop",
    "Display",
    "Error",
    "Read",
    "Write",
    "Sealed",
    "Any",
    "'static",
}


def check_trait_hierarchies() -> CheckResult:
    result = CheckResult("Trait hierarchies")
    trait_re = re.compile(r"^\s*pub\s+trait\s+(\w+)\s*:\s*([^\{]+)", re.MULTILINE)

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            content = path.read_text()
            for m in trait_re.finditer(content):
                trait_name = m.group(1)
                supertraits = m.group(2)
                line_no = content[: m.start()].count("\n") + 1

                # Extract individual trait names
                parts = re.split(r"[+,]", supertraits)
                for part in parts:
                    name = part.strip()
                    # Remove generic args
                    name = re.sub(r"<.*>", "", name).strip()
                    if not name:
                        continue
                    if name in ALLOWED_SUPERTRAITS:
                        continue
                    # Allow fully-qualified standard traits
                    if "::" in name:
                        base = name.split("::")[-1]
                        if base in ALLOWED_SUPERTRAITS:
                            continue
                    result.add(
                        path,
                        line_no,
                        f"trait `{trait_name}` extends `{name}` — "
                        f"traits are capabilities, not taxonomies",
                    )

    return result


# ---------------------------------------------------------------------------
# 7. Effect enum structure — no stringly-typed discriminators
# ---------------------------------------------------------------------------


def check_effect_structure() -> CheckResult:
    result = CheckResult("Effect structure")

    # Find the Effect enum and check its variants don't use serde_json::Value
    # as a primary payload (UiWidget::Custom is the only allowed exception).
    for path in (PROJECT_ROOT / "crates" / "kernel" / "brioche-core" / "src").rglob(
        "*.rs"
    ):
        content = path.read_text()
        lines = content.split("\n")

        for i, line in enumerate(lines):
            if "pub enum Effect" in line:
                # Scan the enum body for forbidden patterns
                brace_depth = 0
                in_enum = False
                for j in range(i, len(lines)):
                    line_text = lines[j]
                    if "{" in line_text:
                        brace_depth += line_text.count("{")
                        in_enum = True
                    if "}" in line_text:
                        brace_depth -= line_text.count("}")
                        if in_enum and brace_depth <= 0:
                            break

                    stripped = line_text.strip()
                    if stripped.startswith("//"):
                        continue

                    # Flag serde_json::Value inside Effect variants.
                    # No exceptions: PHILOSOPHY.md §7.5 bans all stringly-typed holes.
                    if "serde_json::Value" in line_text:
                        result.add(
                            path,
                            j + 1,
                            "Effect variant contains serde_json::Value — use structured types (PHILOSOPHY.md §7.5)",
                        )

                break

    return result


# ---------------------------------------------------------------------------
# 7b. Stringly-typed holes in Effect and hot-path action enums
#     PHILOSOPHY.md §7.5: "No Stringly-Typed Holes in Effect. Structured
#     payloads only."
#
# Scope is intentionally narrow: we only check `Effect` and any enum that
# is an intermediate producer of effects (`InputResult`). Domain enums that
# legitimately carry text (ChatMessage, ToolOutcome, EngineInput, etc.) are
# not flagged. Error enums (PluginError, BriocheError, ErrorDetail) are
# allowed to carry String messages as part of their purpose.
# ---------------------------------------------------------------------------

STRINGLY_TYPED_CHECKED_ENUMS = {"Effect", "InputResult"}


def check_stringly_typed_enums() -> CheckResult:
    result = CheckResult("Stringly-typed enum payloads")
    enum_start_re = re.compile(
        r"^\s*(?:pub(?:\s*\(\s*crate\s*\))?|pub\(crate\))\s+enum\s+(\w+)",
        re.MULTILINE,
    )

    for rel in HOT_PATH_MODULES:
        path = PROJECT_ROOT / rel
        if not path.exists():
            continue

        content = path.read_text()
        lines = content.split("\n")

        for m in enum_start_re.finditer(content):
            enum_name = m.group(1)
            if enum_name not in STRINGLY_TYPED_CHECKED_ENUMS:
                continue

            # Scan enum body for String or &'static str fields in variants.
            brace_depth = 0
            in_enum = False
            start_line = content[: m.start()].count("\n")
            for j in range(start_line, len(lines)):
                line_text = lines[j]
                if "{" in line_text:
                    brace_depth += line_text.count("{")
                    in_enum = True
                if "}" in line_text:
                    brace_depth -= line_text.count("}")
                    if in_enum and brace_depth <= 0:
                        break

                stripped = line_text.strip()
                if stripped.startswith("//"):
                    continue

                if "String" in line_text or "&'static str" in line_text:
                    result.add(
                        path,
                        j + 1,
                        f"`{enum_name}` variant contains String or &'static str — "
                        f"use structured types (PHILOSOPHY.md §7.5)",
                    )

    return result


# ---------------------------------------------------------------------------
# 8. Invariant reference format — if I-XXX is mentioned, it must use Refs:
# ---------------------------------------------------------------------------


def check_invariant_format() -> CheckResult:
    result = CheckResult("Invariant reference format")
    invariant_mention_re = re.compile(r"I-[A-Z][a-zA-Z0-9-]+")

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            content = path.read_text()
            lines = content.split("\n")

            for i, line in enumerate(lines):
                if not line.strip().startswith("///"):
                    continue
                for m in invariant_mention_re.finditer(line):
                    # Check if this line or the surrounding doc block has "Refs:"
                    block = "\n".join(lines[max(0, i - 5) : i + 6])
                    if "Refs:" not in block and "# Invariants" not in block:
                        # Allow invariant mentions inside standard doc sections
                        # (Panics, Complexity, Safety, Errors) where they serve
                        # as cross-references rather than primary citations.
                        if any(
                            sec in block
                            for sec in (
                                "# Panics",
                                "# Complexity",
                                "# Safety",
                                "# Errors",
                            )
                        ):
                            continue
                        result.add(
                            path,
                            i + 1,
                            f"invariant `{m.group(0)}` mentioned without 'Refs:' prefix",
                        )

    return result


# ---------------------------------------------------------------------------
# 9. Module-level docs (!!) for every crate lib.rs
#    PHILOSOPHY.md §4.3: Every crate root and module must have a //! block.
# ---------------------------------------------------------------------------

# ---------------------------------------------------------------------------
# 9b. Module doc visibility accuracy
#     If a module doc claims items are pub(crate), no pub struct/enum/trait
#     (without (crate)) should appear at the top level.
# ---------------------------------------------------------------------------


def check_module_doc_visibility() -> CheckResult:
    result = CheckResult("Module doc visibility accuracy")
    pub_item_re = re.compile(
        r"^\s*pub\s+(struct|enum|trait)\s+\w+",
        re.MULTILINE,
    )

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            if "tests" in path.parts or "benches" in path.parts:
                continue

            content = path.read_text()
            lines = content.split("\n")

            # Check if //! falsely claims top-level items are pub(crate).
            # We look for explicit claims like "These types are `pub(crate)`"
            # but ignore accurate descriptions like "their fields are `pub(crate)`".
            has_pubcrate_claim = False
            pubcrate_claim_re = re.compile(
                r"\b(?:these\s+)?(?:types?|items?|structs?)\s+(?:in\s+\w+\s+)?(?:are|is)\s+`?pub\(crate\)`?",
                re.IGNORECASE,
            )
            for line in lines:
                stripped = line.strip()
                if stripped.startswith("//!") and pubcrate_claim_re.search(stripped):
                    has_pubcrate_claim = True
                    break
                if (
                    stripped
                    and not stripped.startswith("//!")
                    and not stripped.startswith("#![")
                ):
                    break

            if not has_pubcrate_claim:
                continue

            # Look for pub struct/enum/trait that are NOT pub(crate).
            for m in pub_item_re.finditer(content):
                line_no = content[: m.start()].count("\n") + 1
                result.add(
                    path,
                    line_no,
                    "module doc claims items are `pub(crate)` but this item is `pub` — "
                    "fix module doc or reduce visibility",
                )

    return result


# ---------------------------------------------------------------------------
# 9. Module-level docs (!!) for every crate lib.rs
#    PHILOSOPHY.md §4.3: Every crate root and module must have a //! block.
# ---------------------------------------------------------------------------

MODULE_DOC_CRATES = [
    "crates/kernel/brioche-core/src",
    "crates/kernel/brioche-macro/src",
    "crates/kernel/brioche-governance/src",
    "crates/kernel/brioche-governance-default/src",
    "crates/runtime/brioche-shell-runtime/src",
    "crates/runtime/brioche-shell-persistence/src",
    "crates/runtime/brioche-shell-projection/src",
    "crates/providers/brioche-provider-openai/src",
    "crates/tools/brioche-tools-system/src",
    "crates/apps/agent-terminal/src",
    "crates/infra/brioche-reedline/src",
    "crates/infra/cargo-brioche-lint/src",
    "crates/infra/cargo-brioche-lint-invariants/src",
    "crates/ecosystem/brioche-docgen/src",
    "crates/ecosystem/brioche-playground/src",
    "crates/ecosystem/brioche-plugin-kit/src",
    "crates/ecosystem/brioche-std/src",
]


def check_module_docs() -> CheckResult:
    result = CheckResult("Module-level docs (!!)")

    for rel in MODULE_DOC_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            # Skip tests and benches — they don't need module docs.
            if "tests" in path.parts or "benches" in path.parts:
                continue
            if path.name.startswith("fail_") or path.name.startswith("pass_"):
                continue

            content = path.read_text()
            lines = content.split("\n")

            # Look for a //! block before any non-comment, non-blank, non-attribute line.
            has_mod_doc = False
            for line in lines:
                stripped = line.strip()
                if stripped.startswith("//!"):
                    has_mod_doc = True
                    break
                if stripped == "" or stripped.startswith(("#![", "//")):
                    continue
                # Reached code / attributes that aren't module docs
                break

            if not has_mod_doc:
                result.add(path, 1, "missing `//!` module-level documentation block")

    return result


# ---------------------------------------------------------------------------
# 12. Language consistency — English only in all production doc comments
# ---------------------------------------------------------------------------

# These must match as whole words (or near-whole words) to avoid false positives
# on legitimate English terms like "phase", "standard", "profile".
# The accented-character detector catches common French vocabulary that leaks
# into doc comments (invoqué, référence, mécanisme, etc.).
FRENCH_KEYWORDS = [
    ("garde", r"\bgarde\b"),
    ("profondeur", r"\bprofondeur\b"),
    ("sous-routines", r"\bsous-routines\b"),
    ("nettoyage", r"\bnettoyage\b"),
    ("algorithme", r"\balgorithme\b"),
    ("profil", r"\bprofil\b"),  # NOT "profile"
    ("restreint", r"\brestreint\b"),
    ("effets", r"\beffets\b"),
    ("dangereux", r"\bdangereux\b"),
    ("courante", r"\bcourante\b"),
    ("adaptative", r"\badaptative\b"),
    ("adaptatif", r"\badaptatif\b"),
    ("courant", r"\bcourant\b"),
    ("french-accented-word", r"\b\w*[éèêàçôûùï]\w*\b"),
]


def check_language_consistency() -> CheckResult:
    result = CheckResult("Language consistency (English-only)")

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            if "tests" in path.parts or "benches" in path.parts:
                continue

            content = path.read_text()
            lines = content.split("\n")

            for i, line in enumerate(lines):
                stripped = line.strip()
                # Only check doc comments and identifiers (not string literals in code)
                if not stripped.startswith("///") and not stripped.startswith("//!"):
                    continue

                lower = stripped.lower()
                # Skip invariant references like `I-Gov-CowBudget-Adaptative`
                # — spec identifiers are allowed to contain non-English roots.
                if re.search(r"i-[a-z0-9-]+", lower):
                    continue
                for kw, pat in FRENCH_KEYWORDS:
                    if re.search(pat, lower):
                        result.add(
                            path,
                            i + 1,
                            f"French keyword '{kw}' found in doc comment — "
                            f"PHILOSOPHY.md requires English-only prose in all production doc comments",
                        )
                        break

    return result


# ---------------------------------------------------------------------------
# 10. Session !Send / !Sync marker
#     SPECS.md §2.1: Session is !Send and !Sync.
#     Rust stable uses PhantomData<*mut ()> since negative impls are unstable.
# ---------------------------------------------------------------------------


def check_session_send_sync() -> CheckResult:
    result = CheckResult("Session !Send/!Sync marker")
    path = PROJECT_ROOT / "crates" / "kernel" / "brioche-core" / "src" / "types.rs"

    if not path.exists():
        result.add(path, 0, "types.rs not found")
        return result

    content = path.read_text()

    # Accept either PhantomData<*mut ()> or a field named _not_send_sync / NotSendSync
    if "PhantomData<*mut ()>" not in content and "_not_send_sync" not in content:
        result.add(
            path,
            1,
            "Session struct missing `!Send + !Sync` marker (expected PhantomData<*mut ()> "
            "or `_not_send_sync` field)",
        )

    return result


# ---------------------------------------------------------------------------
# 11. critical_state annotation on fundamental governance types
#     SPECS.md §3.2: EpochState, QuarantineState, DepthState, TransitionTraceLog,
#     SupersededTransitionTraceLog, SubRoutineTimerState, HookEffectConstraintState
#     must carry #[brioche(critical_state)].
# ---------------------------------------------------------------------------

FUNDAMENTAL_CRITICAL_TYPES = [
    "EpochState",
    "QuarantineState",
    "DepthState",
    "TransitionTraceLog",
    "SupersededTransitionTraceLog",
    "SubRoutineTimerState",
    "HookEffectConstraintState",
]


def check_critical_state() -> CheckResult:
    result = CheckResult("critical_state annotations")

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            content = path.read_text()
            lines = content.split("\n")

            for i, line in enumerate(lines):
                stripped = line.strip()
                if not stripped.startswith("pub struct "):
                    continue

                struct_name = stripped.split()[2]
                if struct_name not in FUNDAMENTAL_CRITICAL_TYPES:
                    continue

                # Look back up to 10 lines for #[brioche(critical_state)]
                window_start = max(0, i - 10)
                window = "\n".join(lines[window_start:i])
                if "#[brioche(critical_state)]" not in window:
                    result.add(
                        path,
                        i + 1,
                        f"`{struct_name}` is a fundamental governance type and must carry "
                        f"`#[brioche(critical_state)]` (SPECS.md §3.2)",
                    )

    return result


# ---------------------------------------------------------------------------
# 12. TODO / FIXME policy
#     PHILOSOPHY.md §11: Kernel crates forbid TODO/FIXME entirely.
#     Outer crates require attribution: (Sprint N), (#issue), or (name).
# ---------------------------------------------------------------------------

TODO_FORBIDDEN_CRATES = {
    "crates/kernel/brioche-core/src",
    "crates/kernel/brioche-macro/src",
    "crates/kernel/brioche-governance/src",
    "crates/kernel/brioche-governance-default/src",
}

TODO_ATTRIBUTION_RE = re.compile(r"(?:TODO|FIXME)\s*\([^)]+\)", re.IGNORECASE)
TODO_BARE_RE = re.compile(r"\b(?:TODO|FIXME)\b", re.IGNORECASE)


def check_todo_policy() -> CheckResult:
    result = CheckResult("TODO / FIXME policy")

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        is_kernel = rel in TODO_FORBIDDEN_CRATES

        for path in crate_src.rglob("*.rs"):
            if "tests" in path.parts or "benches" in path.parts:
                continue

            content = path.read_text()
            lines = content.split("\n")

            for i, line in enumerate(lines):
                if not TODO_BARE_RE.search(line):
                    continue

                stripped = line.strip()
                # Only inspect comment lines (production code, not string literals).
                if not stripped.startswith(("//", "///", "*")):
                    continue

                if is_kernel:
                    result.add(
                        path,
                        i + 1,
                        "TODO/FIXME is forbidden in kernel crates (PHILOSOPHY.md §11)",
                    )
                    continue

                if not TODO_ATTRIBUTION_RE.search(stripped):
                    result.add(
                        path,
                        i + 1,
                        "bare TODO/FIXME must include attribution: (Sprint N), (#issue), "
                        "or (name) (PHILOSOPHY.md §11)",
                    )

    return result


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

CHECKS = [
    check_hotpath_docs,
    check_panic_safety_docs,
    check_invariant_refs,
    check_extension_type_docs,
    check_determinism,
    check_panic_guards,
    check_vtable_in_core,
    check_trait_hierarchies,
    check_effect_structure,
    check_stringly_typed_enums,
    check_invariant_format,
    check_module_docs,
    check_module_doc_visibility,
    check_session_send_sync,
    check_critical_state,
    check_language_consistency,
    check_todo_policy,
]


def main() -> int:
    print("Brioche Philosophy Check")
    print("=" * 50)

    total = 0
    for check_fn in CHECKS:
        result = check_fn()
        total += result.report()

    print("=" * 50)
    if total == 0:
        print("All philosophy checks passed.")
        return 0
    else:
        print(f"{total} total philosophy violation(s) found.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
