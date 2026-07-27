"""§4 — report validation is separate from provisioned capture.

Two named tests live here:

  * ``test_library_performance_report_validation`` (§4 Dimension 4.1) drives the
    REAL command the acceptance rubric runs, over temporary fixtures. Calling
    the shipped ``bun`` entry point rather than importing its functions is
    deliberate: R3 grades an exit code and a stdout line, and a test that
    imports past the argument parsing and the file loading grades neither.

  * ``test_library_capture_command_is_not_universal_gate`` (§4 Dimension 4.2)
    reads the Makefile fragments and every CI workflow to prove the capture
    target is reachable by hand and by nothing automatic.

The property both exist to protect is that no percentile decides pass/fail.
That is asserted positively — a candidate whose numbers moved enormously is
still valid — rather than by grepping the validator for the absence of a
comparison, because absence of a string is not absence of a behaviour.
"""

from __future__ import annotations

import copy
import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
VALIDATOR = REPO_ROOT / "scripts" / "report-library-performance.ts"
BENCH_MK = REPO_ROOT / "make" / "bench.mk"
WORKFLOWS = REPO_ROOT / ".github" / "workflows"

CAPTURE_TARGET = "capture-library-performance"
OK_LINE = "comparison=valid"

EXIT_OK = 0
EXIT_INVALID = 3
EXIT_USAGE = 2

BASELINE_COMMIT = "1111111111111111111111111111111111111111"
CANDIDATE_COMMIT = "2222222222222222222222222222222222222222"

SHARED_METADATA = {
    "fixture_sha256": "a" * 64,
    "build_profile": "ReleaseSafe",
    "database_version": "17.2",
    "pool_size": 8,
    "replica_count": 1,
    "region_class": "local",
    "warm_state": "warm",
    "concurrency": 4,
}


def _aggregate(**overrides: object) -> dict:
    row = {
        "surface": "tenant_models",
        "stage": "sql",
        "outcome": "ok",
        "cache": "not_applicable",
        "pool_result": "acquired",
        "sample_count": 100,
        "p50_seconds": 0.010,
        "p95_seconds": 0.020,
        "p99_seconds": 0.030,
        "payload_bytes": 4096,
    }
    row.update(overrides)
    return row


def _report(commit: str, **overrides: object) -> dict:
    report = {
        "schema_version": 1,
        "commit_sha": commit,
        "metadata": copy.deepcopy(SHARED_METADATA),
        "aggregates": [
            _aggregate(),
            _aggregate(stage="secret_project", p50_seconds=0.001, p95_seconds=0.002, p99_seconds=0.004),
            _aggregate(stage="pool_wait", pool_result="timeout", outcome="timeout"),
        ],
    }
    report.update(overrides)
    return report


@unittest.skipIf(shutil.which("bun") is None, "bun is not installed")
class LibraryPerformanceReportValidation(unittest.TestCase):
    """test_library_performance_report_validation — §4 Dimension 4.1."""

    def _run(self, baseline: dict, candidate: dict) -> subprocess.CompletedProcess:
        with tempfile.TemporaryDirectory() as tmp:
            base_path = pathlib.Path(tmp) / "baseline.json"
            cand_path = pathlib.Path(tmp) / "candidate.json"
            base_path.write_text(json.dumps(baseline))
            cand_path.write_text(json.dumps(candidate))
            return subprocess.run(
                [
                    "bun",
                    str(VALIDATOR),
                    "--check",
                    "--baseline",
                    str(base_path),
                    "--candidate",
                    str(cand_path),
                ],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
                timeout=120,
            )

    def test_comparable_pair_is_valid(self):
        result = self._run(_report(BASELINE_COMMIT), _report(CANDIDATE_COMMIT))
        self.assertEqual(result.returncode, EXIT_OK, result.stderr)
        self.assertIn(OK_LINE, result.stdout)

    def test_timing_values_never_decide_the_outcome(self):
        """The load-bearing property of the whole workstream.

        The candidate is 100x slower on every percentile and 10x larger. If any
        threshold existed anywhere in this path, this pair would fail. It must
        pass, because the numbers are evidence a human reads and not a gate.
        """
        candidate = _report(CANDIDATE_COMMIT)
        for row in candidate["aggregates"]:
            row["p50_seconds"] *= 100
            row["p95_seconds"] *= 100
            row["p99_seconds"] *= 100
            row["payload_bytes"] *= 10

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_OK, result.stderr)
        self.assertIn(OK_LINE, result.stdout)

    def test_faster_candidate_is_equally_valid(self):
        """The mirror of the case above — no lower bound either."""
        candidate = _report(CANDIDATE_COMMIT)
        for row in candidate["aggregates"]:
            row["p50_seconds"] = 0.0
            row["p95_seconds"] = 0.0
            row["p99_seconds"] = 0.0
            row["payload_bytes"] = 0

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_OK, result.stderr)

    def test_differing_metadata_is_not_comparable(self):
        candidate = _report(CANDIDATE_COMMIT)
        candidate["metadata"]["pool_size"] = SHARED_METADATA["pool_size"] + 1

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_INVALID)
        # Names the field, so an operator does not have to diff two JSON blobs.
        self.assertIn("metadata.pool_size", result.stderr)

    def test_same_commit_is_rejected(self):
        result = self._run(_report(BASELINE_COMMIT), _report(BASELINE_COMMIT))
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn(BASELINE_COMMIT, result.stderr)

    def test_missing_aggregate_key_is_not_comparable(self):
        candidate = _report(CANDIDATE_COMMIT)
        candidate["aggregates"].pop()

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("missing aggregate", result.stderr)

    def test_out_of_order_percentiles_are_rejected(self):
        """Internal consistency, not a threshold: these three numbers cannot
        describe one distribution, whatever their magnitude."""
        candidate = _report(CANDIDATE_COMMIT)
        candidate["aggregates"][0]["p50_seconds"] = 9.0

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("p50 <= p95 <= p99", result.stderr)

    def test_unknown_surface_is_rejected(self):
        """`fleet_detail` names a route that was stripped unconsumed, so a
        report carrying it was produced by something that no longer exists."""
        candidate = _report(CANDIDATE_COMMIT)
        candidate["aggregates"][0]["surface"] = "fleet_detail"

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("surface", result.stderr)

    def test_zero_sample_count_is_rejected(self):
        candidate = _report(CANDIDATE_COMMIT)
        candidate["aggregates"][0]["sample_count"] = 0

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("sample_count", result.stderr)

    def test_duplicate_aggregate_key_is_rejected(self):
        candidate = _report(CANDIDATE_COMMIT)
        candidate["aggregates"].append(copy.deepcopy(candidate["aggregates"][0]))

        result = self._run(_report(BASELINE_COMMIT), candidate)
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("duplicate aggregate key", result.stderr)


@unittest.skipIf(shutil.which("bun") is None, "bun is not installed")
class LibraryPerformanceReportInvocation(unittest.TestCase):
    """The argument and IO paths — every way the command can be MISUSED.

    Separate from the validation cases above because these never reach the
    parser: they decide whether the caller gets a usage error they can act on or
    a stack trace. `EXIT_USAGE` is distinct from `EXIT_INVALID` on purpose, so a
    CI step can tell "you invoked it wrong" from "the reports disagree".
    """

    def _run(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bun", str(VALIDATOR), *args],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=120,
        )

    def test_without_check_flag_reports_usage_not_success(self):
        result = self._run()
        self.assertEqual(result.returncode, EXIT_USAGE)
        self.assertIn("--check", result.stderr)

    def test_missing_baseline_flag_reports_usage(self):
        result = self._run("--check", "--candidate", "x.json")
        self.assertEqual(result.returncode, EXIT_USAGE)
        self.assertIn("--baseline", result.stderr)

    def test_missing_candidate_flag_reports_usage(self):
        result = self._run("--check", "--baseline", "x.json")
        self.assertEqual(result.returncode, EXIT_USAGE)
        self.assertIn("--candidate", result.stderr)

    def test_flag_without_a_value_reports_usage_rather_than_reading_the_next_flag(self):
        """`--baseline --candidate x` must not silently treat `--candidate` as
        the baseline PATH — that would report a confusing file-not-found for a
        filename the caller never typed."""
        result = self._run("--check", "--baseline", "--candidate")
        self.assertEqual(result.returncode, EXIT_USAGE)

    def test_absent_report_file_names_the_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            present = pathlib.Path(tmp) / "baseline.json"
            present.write_text(json.dumps(_report(BASELINE_COMMIT)))
            absent = pathlib.Path(tmp) / "nope.json"
            result = self._run(
                "--check", "--baseline", str(present), "--candidate", str(absent)
            )
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("nope.json", result.stderr)
        self.assertIn("candidate", result.stderr)

    def test_malformed_json_is_reported_as_such_not_as_a_crash(self):
        with tempfile.TemporaryDirectory() as tmp:
            good = pathlib.Path(tmp) / "baseline.json"
            good.write_text(json.dumps(_report(BASELINE_COMMIT)))
            bad = pathlib.Path(tmp) / "candidate.json"
            bad.write_text("{ not json at all")
            result = self._run(
                "--check", "--baseline", str(good), "--candidate", str(bad)
            )
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("not valid JSON", result.stderr)

    def test_a_json_array_is_rejected_rather_than_indexed(self):
        """`[]` is valid JSON and is not a report. Without the object check it
        would reach the field reads and produce `undefined` everywhere."""
        with tempfile.TemporaryDirectory() as tmp:
            good = pathlib.Path(tmp) / "baseline.json"
            good.write_text(json.dumps(_report(BASELINE_COMMIT)))
            arr = pathlib.Path(tmp) / "candidate.json"
            arr.write_text("[]")
            result = self._run(
                "--check", "--baseline", str(good), "--candidate", str(arr)
            )
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("JSON object", result.stderr)

    def test_wrong_schema_version_is_rejected(self):
        candidate = _report(CANDIDATE_COMMIT)
        candidate["schema_version"] = 2
        with tempfile.TemporaryDirectory() as tmp:
            good = pathlib.Path(tmp) / "baseline.json"
            good.write_text(json.dumps(_report(BASELINE_COMMIT)))
            bad = pathlib.Path(tmp) / "candidate.json"
            bad.write_text(json.dumps(candidate))
            result = self._run(
                "--check", "--baseline", str(good), "--candidate", str(bad)
            )
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("schema_version", result.stderr)

    def test_empty_aggregates_is_rejected_as_vacuous(self):
        """A report with no aggregates compares equal to any other report with
        no aggregates — it would pass comparability while proving nothing."""
        candidate = _report(CANDIDATE_COMMIT)
        candidate["aggregates"] = []
        with tempfile.TemporaryDirectory() as tmp:
            good = pathlib.Path(tmp) / "baseline.json"
            good.write_text(json.dumps(_report(BASELINE_COMMIT)))
            bad = pathlib.Path(tmp) / "candidate.json"
            bad.write_text(json.dumps(candidate))
            result = self._run(
                "--check", "--baseline", str(good), "--candidate", str(bad)
            )
        self.assertEqual(result.returncode, EXIT_INVALID)
        self.assertIn("must not be empty", result.stderr)

    def test_nan_percentile_is_rejected(self):
        """`typeof NaN === "number"` passes a naive type check, and every
        comparison against NaN is false — so an unguarded NaN would sail through
        the ordering check as "consistent"."""
        with tempfile.TemporaryDirectory() as tmp:
            good = pathlib.Path(tmp) / "baseline.json"
            good.write_text(json.dumps(_report(BASELINE_COMMIT)))
            bad = pathlib.Path(tmp) / "candidate.json"
            # json.dumps emits bare NaN, which Bun's JSON.parse rejects; write
            # the literal the way a hand-rolled emitter would.
            bad.write_text(
                json.dumps(_report(CANDIDATE_COMMIT)).replace('"p95_seconds": 0.02', '"p95_seconds": 1e999', 1)
            )
            result = self._run(
                "--check", "--baseline", str(good), "--candidate", str(bad)
            )
        self.assertEqual(result.returncode, EXIT_INVALID)


class LibraryCaptureCommandIsNotUniversalGate(unittest.TestCase):
    """test_library_capture_command_is_not_universal_gate — §4 Dimension 4.2."""

    def test_capture_target_exists_and_is_phony(self):
        text = BENCH_MK.read_text()
        self.assertIn(f"{CAPTURE_TARGET}:", text)
        phony = [ln for ln in text.splitlines() if ln.startswith(".PHONY:")]
        self.assertTrue(
            any(CAPTURE_TARGET in ln for ln in phony),
            "capture target must be .PHONY — it produces no file of its own name",
        )

    def test_capture_is_absent_from_every_ci_workflow(self):
        offenders = [
            path.name
            for path in sorted(WORKFLOWS.glob("*.yml"))
            if CAPTURE_TARGET in path.read_text()
        ]
        self.assertEqual(
            offenders,
            [],
            f"{CAPTURE_TARGET} is provisioned-only and must not run in CI; found in: {offenders}",
        )

    def test_capture_is_not_reachable_from_an_aggregate_target(self):
        """Absent from CI is not enough — a target that `make lint-all` or
        `make test` depends on would run everywhere without naming itself in a
        workflow."""
        for mk in sorted((REPO_ROOT / "make").glob("*.mk")):
            for line in mk.read_text().splitlines():
                stripped = line.strip()
                if stripped.startswith("#") or stripped.startswith(".PHONY:"):
                    continue
                if ":" not in line or line.startswith("\t"):
                    continue
                target, _, prereqs = line.partition(":")
                if target.strip() == CAPTURE_TARGET:
                    continue
                self.assertNotIn(
                    CAPTURE_TARGET,
                    prereqs.split("#")[0],
                    f"{mk.name}: '{target.strip()}' depends on {CAPTURE_TARGET}",
                )

    def test_capture_recipe_gates_on_structure_and_not_on_values(self):
        """The recipe may fail for setup, execution, schema, sanitization, or
        malformed output — never because a percentile moved. A threshold would
        have to compare a p-value somewhere in the recipe body."""
        text = BENCH_MK.read_text()
        start = text.index(f"{CAPTURE_TARGET}:")
        recipe = text[start:]
        for forbidden in ("p50", "p95", "p99", "MAX_P95_MS", "payload_bytes"):
            self.assertNotIn(
                forbidden,
                recipe.split("\n\n")[0],
                f"capture recipe references {forbidden}; values must not gate",
            )

    def test_capture_requires_both_refs(self):
        text = BENCH_MK.read_text()
        start = text.index(f"{CAPTURE_TARGET}:")
        recipe = text[start:].split("\n\n")[0]
        self.assertIn("BASELINE_REF", recipe)
        self.assertIn("CANDIDATE_REF", recipe)


if __name__ == "__main__":
    unittest.main()
