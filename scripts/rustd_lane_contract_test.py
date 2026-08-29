#!/usr/bin/env python3
"""Static contracts for the canonical Rust coverage orchestration."""
from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
LANE = ROOT / "make" / "test-integration-rustd.mk"


class CoverageLane(unittest.TestCase):
    def setUp(self) -> None:
        self.recipe = LANE.read_text()

    def test_coverage_lane_reuses_the_instrumented_migration_build(self) -> None:
        coverage = self.recipe.split("test-coverage-rustd:", 1)[1]
        self.assertIn("cargo llvm-cov run", coverage)
        self.assertIn("cargo llvm-cov --workspace", coverage)
        self.assertIn("--no-clean", coverage)
        self.assertNotIn("_migrate-test-db", coverage.split("\n\n", 1)[0])

    def test_coverage_lane_discards_stale_instrumented_binaries(self) -> None:
        coverage = self.recipe.split("test-coverage-rustd:", 1)[1]
        self.assertIn("cargo llvm-cov clean --workspace", coverage)
        self.assertNotIn("cargo clean", coverage)

    def test_coverage_lane_rejects_an_under_target_report(self) -> None:
        coverage = self.recipe.split("test-coverage-rustd:", 1)[1]
        self.assertIn("--fail-under-lines 100", coverage)


if __name__ == "__main__":
    unittest.main()
