#!/usr/bin/env python3
"""Behaviour and rejection tests for Rust lane benchmark comparisons."""
from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from rustd_lane_benchmark import (
    Evidence,
    EvidenceError,
    artifact_bytes,
    compare,
    coverage_lines,
    run_once,
)


def evidence(path: Path, walls: list[float], *, total: int = 25_961, tests: int = 1_629) -> Evidence:
    """Write one explicit equivalent-run fixture and read it through production parsing."""
    path.write_text(
        json.dumps(
            {
                "schema": 1,
                "toolchain": "rustc 1.98.0",
                "runner": "ubuntu-latest-x64",
                "reset_contract": "compose-postgres-redis-clean",
                "runs": [
                    {
                        "wall_seconds": wall,
                        "artifact_bytes": 6_000_000_000,
                        "test_count": tests,
                        "covered_lines": total,
                        "total_lines": total,
                    }
                    for wall in walls
                ],
            }
        ),
        encoding="utf-8",
    )
    return Evidence.read(path)


class LaneBenchmark(unittest.TestCase):
    def test_equivalent_medians_at_the_budget_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = evidence(root / "before.json", [500.0, 520.0, 510.0])
            after = evidence(root / "after.json", [408.0, 400.0, 404.0])
            passed, summary = compare(before, after)
            self.assertTrue(passed)
            self.assertIn("ratio 0.792", summary)
            self.assertIn("1629", summary)

    def test_a_regression_above_the_budget_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = evidence(root / "before.json", [500.0, 500.0, 500.0])
            after = evidence(root / "after.json", [401.0, 410.0, 405.0])
            passed, _summary = compare(before, after)
            self.assertFalse(passed)

    def test_a_changed_denominator_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = evidence(root / "before.json", [500.0, 510.0, 520.0])
            after = evidence(root / "after.json", [390.0, 400.0, 410.0], total=25_960)
            with self.assertRaisesRegex(EvidenceError, "denominator"):
                compare(before, after)

    def test_fewer_than_three_runs_cannot_support_a_median_claim(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "short.json"
            with self.assertRaisesRegex(EvidenceError, "at least three"):
                evidence(path, [500.0, 510.0])

    def test_a_lower_test_count_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = evidence(root / "before.json", [500.0, 510.0, 520.0])
            after = evidence(root / "after.json", [390.0, 400.0, 410.0], tests=1_628)
            with self.assertRaisesRegex(EvidenceError, "fewer tests"):
                compare(before, after)

    def test_evidence_round_trips_through_the_stable_schema(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = evidence(root / "input.json", [500.0, 510.0, 520.0])
            output = root / "nested" / "output.json"
            original.write(output)
            self.assertEqual(Evidence.read(output), original)


class Observation(unittest.TestCase):
    def test_lcov_counts_unique_lines_and_merges_duplicate_records(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "lcov.info"
            report.write_text(
                "SF:a.rs\nDA:1,0\nDA:2,1\nend_of_record\n"
                "SF:a.rs\nDA:1,3\nDA:2,1\nend_of_record\n"
                "SF:b.rs\nDA:1,0\nend_of_record\n",
                encoding="utf-8",
            )
            self.assertEqual(coverage_lines(report), (2, 3))

    def test_artifact_size_uses_allocated_files_without_following_links(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = root / "payload"
            payload.write_bytes(b"x" * 4_096)
            link = root / "link"
            link.symlink_to(payload)
            expected = payload.stat().st_blocks * 512 + link.lstat().st_blocks * 512
            self.assertEqual(artifact_bytes(root), expected)

    def test_a_successful_lane_records_direct_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts = root / "target"
            artifacts.mkdir()
            (artifacts / "object").write_bytes(b"x" * 4_096)
            report = root / "lcov.info"
            report.write_text("SF:a.rs\nDA:1,1\nDA:2,0\n", encoding="utf-8")
            command = [
                os.fsdecode(os.environ.get("PYTHON", os.sys.executable)),
                "-c",
                "print('test result: ok. 7 passed; 0 failed; 0 ignored')",
            ]
            with open(os.devnull, "w", encoding="utf-8") as sink:
                with patch("rustd_lane_benchmark.time.monotonic", side_effect=[2.0, 5.5]):
                    run = run_once(command, root, artifacts, report, sink)
            self.assertEqual(run.wall_seconds, 3.5)
            self.assertEqual(run.test_count, 7)
            self.assertEqual((run.covered_lines, run.total_lines), (1, 2))

    def test_a_failing_lane_cannot_be_recorded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with open(os.devnull, "w", encoding="utf-8") as sink:
                with self.assertRaisesRegex(EvidenceError, "status 9"):
                    run_once(
                        [os.sys.executable, "-c", "raise SystemExit(9)"],
                        root,
                        root,
                        root / "missing.info",
                        sink,
                    )


if __name__ == "__main__":
    unittest.main()
