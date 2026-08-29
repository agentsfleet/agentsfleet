#!/usr/bin/env python3
"""Dimension 8.2 — the lane propagates a failure, and refuses a silent no-op.

The guard these cover is the one that cannot be checked by running the lane
successfully: a green run proves the happy path and says nothing about whether
a red one would have been noticed. So the two failure modes are driven directly.

Run: python3 -m unittest discover -s scripts -t scripts -p '*_test.py'
"""
from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

from rustd_lane_result import (
    PhaseClock,
    child_status,
    passed_total,
    phase_report,
    run,
    verdict,
    verdict_from_total,
)

# One binary's summary line, as cargo prints it.
ONE_BINARY = "test result: ok. 12 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out\n"

# What a workspace run looks like: many binaries, most of them empty.
WORKSPACE = (
    "   Running unittests src/lib.rs\n"
    "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    "   Running tests/integration_readyz.rs\n"
    "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    "   Running tests/integration_migrate.rs\n"
    "test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
)

# A selection that matched nothing. Cargo exits 0 for this.
MATCHED_NOTHING = (
    "   Running tests/integration_readyz.rs\n"
    "test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out\n"
)


class PassedTotal(unittest.TestCase):
    def test_sums_across_every_binary(self):
        self.assertEqual(passed_total(WORKSPACE), 8)

    def test_reads_a_single_binary(self):
        self.assertEqual(passed_total(ONE_BINARY), 12)

    def test_is_zero_when_nothing_ran(self):
        self.assertEqual(passed_total(MATCHED_NOTHING), 0)

    def test_is_zero_for_output_with_no_summary_at_all(self):
        self.assertEqual(passed_total("error: could not compile\n"), 0)


class Verdict(unittest.TestCase):
    def test_a_failing_cargo_run_propagates_its_own_status(self):
        code, message = verdict(WORKSPACE, status=101)
        self.assertEqual(code, 101, "the lane must exit with cargo's status, not 1")
        self.assertIn("failed", message)

    def test_a_failure_wins_even_when_tests_passed_before_it(self):
        # The seeded-failure case: some binaries pass, one does not, cargo
        # exits non-zero. A tally-only check would call this a pass.
        code, _message = verdict(WORKSPACE, status=1)
        self.assertEqual(code, 1)

    def test_a_run_that_matched_nothing_is_a_failure(self):
        code, message = verdict(MATCHED_NOTHING, status=0)
        self.assertEqual(code, 1, "0 passing tests exits 0 in cargo and must not here")
        self.assertIn("did not run", message)

    def test_a_real_run_passes_and_reports_its_count(self):
        code, message = verdict(WORKSPACE, status=0)
        self.assertEqual(code, 0)
        self.assertIn("8 tests", message)

    def test_the_streaming_path_classifies_its_in_memory_total(self):
        self.assertEqual(verdict_from_total(3, 0), (0, "passed (3 tests)"))
        code, message = verdict_from_total(0, 0)
        self.assertEqual(code, 1)
        self.assertIn("did not run", message)


class CommandRunner(unittest.TestCase):
    def test_streams_and_tallies_without_a_status_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            status, passed, _clock = run(
                [sys.executable, "-c", f"print({ONE_BINARY!r})"],
                root,
                root / "lane.log",
            )

            self.assertEqual(status, 0)
            self.assertEqual(passed, 12)
            self.assertIn("12 passed", (root / "lane.log").read_text())

    def test_an_unwritable_tally_cannot_replace_the_child_status(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            status, passed, _clock = run(
                [
                    sys.executable,
                    "-c",
                    f"print({ONE_BINARY!r}); raise SystemExit(23)",
                ],
                root,
                root,
            )

            self.assertEqual(status, 23)
            self.assertEqual(passed, 12)

    def test_a_signal_uses_the_shells_status_convention(self):
        self.assertEqual(child_status(-15), 143)


class Phases(unittest.TestCase):
    """The compile/execute split a speed claim is graded on."""

    @staticmethod
    def _clock(ticks):
        """A PhaseClock reading a scripted sequence of monotonic values."""
        remaining = iter(ticks)
        return PhaseClock(now=lambda: next(remaining))

    def test_the_finished_line_splits_compile_from_execution(self):
        # started=0, Finished at 30, phases() reads 100 then 100.
        clock = self._clock([0.0, 30.0, 100.0, 100.0])
        clock.mark("   Compiling afd_core v0.27.0\n")
        clock.mark("    Finished `test` profile [unoptimized] target(s) in 30s\n")

        self.assertEqual(clock.phases(), (30.0, 70.0, 100.0))

    def test_only_the_first_finished_line_is_the_boundary(self):
        # A second `Finished` must not move the boundary and shrink the build.
        clock = self._clock([0.0, 30.0, 100.0, 100.0])
        clock.mark("    Finished `test` profile in 30s\n")
        clock.mark("    Finished `bench` profile in 5s\n")

        compile_s, _tests_s, _total = clock.phases()
        self.assertEqual(compile_s, 30.0)

    def test_a_build_that_never_finished_reports_no_split(self):
        # Inventing a zero compile phase would claim the build was free.
        clock = self._clock([0.0, 12.0])
        clock.mark("error: could not compile `afd_api`\n")

        self.assertEqual(clock.phases(), (None, None, 12.0))

    def test_the_report_line_carries_every_number_it_measured(self):
        clock = self._clock([0.0, 30.0, 100.0, 100.0])
        clock.mark("    Finished `test` profile in 30s\n")

        line = phase_report(clock, 212)

        self.assertIn("compile_s=30.0", line)
        self.assertIn("tests_s=70.0", line)
        self.assertIn("total_s=100.0", line)
        self.assertIn("tests=212", line)

    def test_an_unsplit_run_says_so_rather_than_reporting_zero(self):
        clock = self._clock([0.0, 12.0])

        line = phase_report(clock, 0)

        self.assertIn("compile_s=unsplit", line)
        self.assertIn("tests_s=unsplit", line)
        self.assertIn("total_s=12.0", line)


if __name__ == "__main__":
    unittest.main()
