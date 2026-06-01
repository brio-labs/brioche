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


def check_hotpath_docs() -> CheckResult:
    result = CheckResult("Hot-path documentation")
    pub_fn_re = re.compile(
        r"^\s*pub(?:\s*\([^)]*\)|\s+unsafe)?\s+fn\s+(\w+)", re.MULTILINE
    )

    for rel in HOT_PATH_MODULES:
        path = PROJECT_ROOT / rel
        if not path.exists():
            result.add(path, 0, "file does not exist — skipping")
            continue

        content = path.read_text()
        lines = content.split("\n")

        for m in pub_fn_re.finditer(content):
            fn_name = m.group(1)
            pos = m.start()
            line_no = content[:pos].count("\n") + 1

            # Walk backwards from the function to collect its doc block.
            # Skip blank lines and attributes.
            start_line = line_no - 1  # 0-based
            doc_lines: list[str] = []
            for idx in range(start_line, -1, -1):
                stripped = lines[idx].strip()
                if stripped.startswith("///"):
                    doc_lines.insert(0, stripped)
                elif stripped == "" or stripped.startswith("#"):
                    continue
                else:
                    break

            # Only flag functions that *have* docs but lack complexity notes.
            # Missing docs is a broader lint issue, not hot-path specific.
            if not doc_lines:
                continue

            doc_block = "\n".join(doc_lines)
            if not any(kw in doc_block for kw in COMPLEXITY_KEYWORDS):
                result.add(
                    path,
                    line_no,
                    f"pub fn `{fn_name}` missing complexity/budget note",
                )

    return result


# ---------------------------------------------------------------------------
# 2. Invariant references on pub items in Core & Governance
# ---------------------------------------------------------------------------

INVARIANT_CRATES = [
    "crates/kernel/brioche-core/src",
    "crates/kernel/brioche-governance/src",
]

INVARIANT_PATTERNS = [
    "Refs: I-",
    "# Invariants",
    "# Invariant",
]


def check_invariant_refs() -> CheckResult:
    result = CheckResult("Invariant references")
    # Only check pub fn — structs/enums/traits often have module-level docs.
    pub_fn_re = re.compile(
        r"^\s*pub(?:\s*\([^)]*\)|\s+unsafe)?\s+fn\s+(\w+)",
        re.MULTILINE,
    )

    for rel in INVARIANT_CRATES:
        crate_src = PROJECT_ROOT / rel
        if not crate_src.exists():
            continue

        for path in crate_src.rglob("*.rs"):
            content = path.read_text()
            lines = content.split("\n")

            for m in pub_fn_re.finditer(content):
                fn_name = m.group(1)
                pos = m.start()
                line_no = content[:pos].count("\n") + 1

                # Collect preceding doc block
                start_line = line_no - 1
                doc_lines: list[str] = []
                for idx in range(start_line, -1, -1):
                    stripped = lines[idx].strip()
                    if stripped.startswith("///"):
                        doc_lines.insert(0, stripped)
                    elif stripped == "" or stripped.startswith("#"):
                        continue
                    else:
                        break

                # Only check functions that already have docs.
                if not doc_lines:
                    continue

                doc_block = "\n".join(doc_lines)
                if not any(pat in doc_block for pat in INVARIANT_PATTERNS):
                    result.add(
                        path,
                        line_no,
                        f"pub fn `{fn_name}` doc missing invariant ref "
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

        # Find derive lines mentioning BriocheExtensionType
        for i, line in enumerate(lines):
            if "BriocheExtensionType" not in line or not line.strip().startswith("#"):
                continue

            # Walk up to find the struct definition and its doc block
            struct_idx = None
            for j in range(i + 1, len(lines)):
                if lines[j].strip().startswith("pub struct "):
                    struct_idx = j
                    break
                if lines[j].strip() and not lines[j].strip().startswith("#"):
                    break

            if struct_idx is None:
                continue

            # Collect doc block above the derive
            doc_lines: list[str] = []
            for idx in range(i - 1, -1, -1):
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
# 5. Panic guards in Core (backup to clippy workspace lints)
# ---------------------------------------------------------------------------

PANIC_PATTERNS = [
    (re.compile(r"\.unwrap\(\)"), "unwrap()"),
    (re.compile(r"\.expect\("), "expect(...)"),
    (re.compile(r"\bpanic!\("), "panic!(...)"),
]


def check_panic_guards() -> CheckResult:
    result = CheckResult("Panic guards (Core)")
    core_src = PROJECT_ROOT / "crates" / "kernel" / "brioche-core" / "src"

    for path in core_src.rglob("*.rs"):
        if "tests" in path.parts:
            continue

        content = path.read_text()
        lines = content.split("\n")

        for pat, desc in PANIC_PATTERNS:
            for m in pat.finditer(content):
                line_no = content[: m.start()].count("\n") + 1
                line = lines[line_no - 1].strip()
                if line.startswith("//"):
                    continue
                # Allow expect in non-production paths if explicitly justified
                result.add(path, line_no, f"forbidden: {desc}")

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
    for path in (PROJECT_ROOT / "crates" / "kernel" / "brioche-core" / "src").rglob("*.rs"):
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

                    # Flag serde_json::Value inside Effect variants
                    # (exclude Custom catch-all which is documented)
                    if (
                        "serde_json::Value" in line_text
                        and "Custom" not in lines[max(0, j - 3) : j]
                    ):
                        result.add(
                            path,
                            j + 1,
                            "Effect variant contains serde_json::Value — use structured types",
                        )

                break

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

CRATE_LIB_FILES = [
    "crates/kernel/brioche-core/src/lib.rs",
    "crates/ecosystem/brioche-docgen/src/lib.rs",
    "crates/kernel/brioche-governance-default/src/lib.rs",
    "crates/kernel/brioche-governance/src/lib.rs",
    "crates/kernel/brioche-macro/src/lib.rs",
    "crates/ecosystem/brioche-playground/src/lib.rs",
    "crates/ecosystem/brioche-plugin-kit/src/lib.rs",
    "crates/brioche-plugin-template/src/lib.rs",
    "crates/providers/brioche-provider-openai/src/lib.rs",
    "crates/runtime/brioche-shell-persistence/src/lib.rs",
    "crates/runtime/brioche-shell-projection/src/lib.rs",
    "crates/runtime/brioche-shell-runtime/src/lib.rs",
    "crates/ecosystem/brioche-std/src/lib.rs",
    "crates/tools/brioche-tool-fetch/src/lib.rs",
    "crates/tools/brioche-tool-listdir/src/lib.rs",
    "crates/tools/brioche-tool-readfile/src/lib.rs",
    "crates/tools/brioche-tool-shell/src/lib.rs",
    "crates/tools/brioche-tool-writefile/src/lib.rs",
    "crates/apps/agent-terminal/src/main.rs",
    "crates/infra/brioche-reedline/src/lib.rs",
    "crates/infra/cargo-brioche-lint/src/main.rs",
    "crates/infra/cargo-brioche-lint-invariants/src/main.rs",
]


def check_module_docs() -> CheckResult:
    result = CheckResult("Module-level docs (!!)")

    for rel in CRATE_LIB_FILES:
        path = PROJECT_ROOT / rel
        if not path.exists():
            result.add(path, 0, "file does not exist")
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
# Main
# ---------------------------------------------------------------------------

CHECKS = [
    check_hotpath_docs,
    check_invariant_refs,
    check_extension_type_docs,
    check_determinism,
    check_panic_guards,
    check_trait_hierarchies,
    check_effect_structure,
    check_invariant_format,
    check_module_docs,
    check_session_send_sync,
    check_critical_state,
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
