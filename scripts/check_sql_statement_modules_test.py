#!/usr/bin/env python3
"""Tests for check_sql_statement_modules.

The checker's denominator is the whole point of it — a ratio computed over the
wrong set of files is worse than no ratio, because it looks authoritative. So
these pin what counts, what is excluded, and where the threshold flips.
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import check_sql_statement_modules as checker

STATEMENTS = "\n".join(
    f'    \\\\SELECT col_{i} FROM core.things WHERE id = $1' for i in range(4)
)


def write(root: Path, rel: str, body: str) -> Path:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8")
    return path


class DenominatorTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name) / "src"
        self.addCleanup(self._tmp.cleanup)

    def test_inline_statements_count_against_adoption(self) -> None:
        write(self.root, "agentsfleetd/state/thing_store.zig", STATEMENTS)
        extracted, total = checker.adoption(checker.survey(self.root))
        self.assertEqual((0, 4), (extracted, total))

    def test_statement_module_counts_as_extracted(self) -> None:
        write(self.root, "agentsfleetd/state/sql.zig", STATEMENTS)
        extracted, total = checker.adoption(checker.survey(self.root))
        self.assertEqual((4, 4), (extracted, total))

    def test_domain_below_minimum_is_not_in_the_denominator(self) -> None:
        # Two statements: a sibling module would be churn, so the domain is not
        # counted at all rather than counted as a failure.
        body = "\n".join(['    \\\\SELECT a FROM core.t', '    \\\\SELECT b FROM core.t'])
        write(self.root, "agentsfleetd/state/small.zig", body)
        self.assertEqual((0, 0), checker.adoption(checker.survey(self.root)))

    def test_tests_and_fixtures_are_excluded(self) -> None:
        write(self.root, "agentsfleetd/state/thing_test.zig", STATEMENTS)
        write(self.root, "agentsfleetd/db/test_fixtures.zig", STATEMENTS)
        self.assertEqual((0, 0), checker.adoption(checker.survey(self.root)))

    def test_migration_bootstrap_and_metering_are_excluded(self) -> None:
        write(self.root, "agentsfleetd/db/pool_migrations.zig", STATEMENTS)
        write(self.root, "agentsfleetd/fleet/renewal.zig", STATEMENTS)
        write(self.root, "agentsfleetd/fleet/renewal_settle.zig", STATEMENTS)
        self.assertEqual((0, 0), checker.adoption(checker.survey(self.root)))

    def test_files_outside_the_data_access_layer_are_not_counted(self) -> None:
        write(self.root, "agentsfleetd/observability/metrics.zig", STATEMENTS)
        self.assertEqual((0, 0), checker.adoption(checker.survey(self.root)))


class ConstantSurfaceTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name) / "src"
        self.addCleanup(self._tmp.cleanup)

    def test_constants_only_is_a_constant_surface(self) -> None:
        path = write(self.root, "agentsfleetd/state/sql.zig", f"pub const Q =\n{STATEMENTS}\n;\n")
        self.assertTrue(checker.is_constant_surface(path))

    def test_a_function_disqualifies_it(self) -> None:
        path = write(self.root, "agentsfleetd/state/sql.zig", "pub fn build() []const u8 { return \"\"; }\n")
        self.assertFalse(checker.is_constant_surface(path))

    def test_an_allocation_disqualifies_it(self) -> None:
        path = write(self.root, "agentsfleetd/state/sql.zig", "const x = allocator.dupe(u8, \"q\");\n")
        self.assertFalse(checker.is_constant_surface(path))

    def test_the_word_in_a_comment_does_not_disqualify_it(self) -> None:
        # Statement modules explain themselves; prose mentioning the allocator
        # must not read as an allocation.
        body = "// callers own the result; no allocator is involved here\npub const Q = \"SELECT 1\";\n"
        path = write(self.root, "agentsfleetd/state/sql.zig", body)
        self.assertTrue(checker.is_constant_surface(path))


class ThresholdTest(unittest.TestCase):
    def test_ratio_is_reported_as_extracted_over_total(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "src"
            write(root, "agentsfleetd/state/sql.zig", STATEMENTS)
            write(root, "agentsfleetd/fleet/inline.zig", STATEMENTS)
            extracted, total = checker.adoption(checker.survey(root))
            self.assertEqual((4, 8), (extracted, total))
            self.assertAlmostEqual(50.0, (extracted / total) * 100.0)


if __name__ == "__main__":
    unittest.main()
