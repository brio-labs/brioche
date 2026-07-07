#!/usr/bin/env python3
"""Unit tests for philosophy-check helper rules."""

import importlib.util
import unittest
from pathlib import Path

CHECKER_PATH = Path(__file__).with_name("philosophy-check.py")
SPEC = importlib.util.spec_from_file_location("philosophy_check", CHECKER_PATH)
philosophy_check = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(philosophy_check)


class LargeFileRuleTests(unittest.TestCase):
    def test_logic_counter_preserves_existing_attribute_counting(self) -> None:
        lines = [
            "//! module docs",
            "",
            "use std::fmt;",
            "pub mod child;",
            "#[derive(Clone)]",
            "pub struct Visible;",
            "impl Visible {",
            "    pub fn new() -> Self { Self }",
            "}",
        ]

        self.assertEqual(philosophy_check._count_reviewable_logic_lines(lines), 5)

    def test_large_file_exemption_rejects_missing_and_generic_reasons(self) -> None:
        self.assertFalse(philosophy_check._large_file_exemption_valid(None))
        self.assertFalse(philosophy_check._large_file_exemption_valid("legacy"))
        self.assertFalse(philosophy_check._large_file_exemption_valid("large file"))
        self.assertFalse(philosophy_check._large_file_exemption_valid("short reason"))

    def test_large_file_exemption_accepts_architectural_reason(self) -> None:
        reason = (
            "Keeps stream parsing and terminal-event invariants together "
            "until the provider client is split by cohesive concern."
        )

        self.assertTrue(philosophy_check._large_file_exemption_valid(reason))

    def test_known_large_file_exemptions_have_actionable_reasons(self) -> None:
        for path, reason in philosophy_check.LARGE_FILE_EXEMPTIONS.items():
            with self.subTest(path=path):
                self.assertTrue(philosophy_check._large_file_exemption_valid(reason))


if __name__ == "__main__":
    unittest.main()
