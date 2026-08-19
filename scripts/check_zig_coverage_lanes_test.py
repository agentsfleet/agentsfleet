#!/usr/bin/env python3
"""Drive the two Zig coverage lanes and assert what each one executes.

`make test-coverage-zig` and `make test-integration` own disjoint halves of the
coverage union; neither may reach into the other's half. That is a claim about
recipes, so these tests run the real recipes — against stub `zig` and `kcov`
binaries, so nothing compiles and no suite runs — and read back which binaries
each lane handed the instrument. The shared harness pieces (the lifecycle run
marker, the executable-stub helper) come from `check_zig_test_lanes_test`. The
workflow-wiring class at the end holds CI to the same one-owner rule, asserted
over the workflow sources that invoke these recipes.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

from check_zig_test_lanes_test import LIFECYCLE_MARKER_ECHO, ROOT, write_executable


class LaneCase(unittest.TestCase):
    """Drive the real Make recipes against stub `zig` and `kcov` binaries.

    Nothing here compiles or runs a test suite. What is under test is the lane:
    which binaries it hands kcov, what it does with a component that fails, and
    what it refuses to grade. Stubbing is what makes those assertions cheap
    enough to keep; driving the real recipe is what stops them drifting from the
    file they describe.
    """

    def setUp(self) -> None:
        self.raw = tempfile.TemporaryDirectory()
        self.addCleanup(self.raw.cleanup)
        self.temp = Path(self.raw.name)
        self.tool_dir = self.temp / "bin"
        self.tool_dir.mkdir()
        self.binary_log = self.temp / "binaries.log"
        # TEST_INFRA=provided only checks the cert is non-empty, which is the
        # whole point: these tests are about lane arithmetic, not provisioning.
        self.cert = self.temp / "redis-ca.crt"
        self.cert.write_text("stub cert: TEST_INFRA=provided only checks it is non-empty\n")

    def install_stubs(self, kcov_body: str, lifecycle_ran: bool = True) -> None:
        write_executable(self.tool_dir / "zig", "#!/bin/sh\nexit 0\n")
        # Record every binary kcov is handed. kcov's argument order is flags,
        # then the output directory, then the binary — so the second non-flag
        # argument is what a test asserts a lane executed.
        preamble = (
            "#!/bin/sh\n"
            "kcov_seen=0\n"
            'for kcov_arg in "$@"; do\n'
            '  case "$kcov_arg" in --*) continue;; esac\n'
            "  kcov_seen=$((kcov_seen+1))\n"
            f'  [ "$kcov_seen" -eq 2 ] && printf \'%s\\n\' "$kcov_arg" >> "{self.binary_log}"\n'
            "done\n"
        )
        stub = preamble + textwrap.dedent(kcov_body).replace("#!/bin/sh\n", "", 1)
        if lifecycle_ran:
            stub += LIFECYCLE_MARKER_ECHO
        write_executable(self.tool_dir / "kcov", stub)

    def run_target(self, target: str, *extra: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["PATH"] = f"{self.tool_dir}:/usr/bin:/bin:/usr/sbin:/sbin"
        return subprocess.run(
            [
                "make", target,
                f"ZIG_COVERAGE_DIR={self.temp / 'coverage'}",
                f"ZIG_EVIDENCE_DIR={self.temp / 'evidence'}",
                f"ZIG_GLOBAL_CACHE_DIR={self.temp / 'global'}",
                f"ZIG_LOCAL_CACHE_DIR={self.temp / 'local'}",
                # Redirected for the same reason as the coverage directory: the
                # default path is the one a real run publishes and CI reads, and
                # a stubbed run must not overwrite it.
                f"ZIG_COVERAGE_SUMMARY_FILE={self.temp / 'zig-coverage.txt'}",
                "TEST_INFRA=provided",
                "KEEP_TEST_STATE=1",
                f"TEST_REDIS_TLS_CA_CERT={self.cert}",
                *extra,
            ],
            cwd=ROOT, env=env, text=True, capture_output=True, check=False,
        )

    def binaries_measured(self) -> list[str]:
        if not self.binary_log.exists():
            return []
        return [line for line in self.binary_log.read_text().splitlines() if line]

    def output(self, result: subprocess.CompletedProcess[str]) -> str:
        return result.stdout + result.stderr


# What kcov would have written: one covered line and nine uncovered, so the
# union grades at 10% and any floor above that is red.
SPARSE_REPORT = """\
#!/bin/sh
for arg in "$@"; do case "$arg" in --*) ;; *) out=$arg; break;; esac; done
mkdir -p "$out"
{
  printf '<coverage><packages><package><classes>\\n'
  printf '<class filename="a.zig"><lines>\\n'
  printf '<line number="1" hits="1"/>\\n'
  i=2
  while [ "$i" -le 10 ]; do
    printf '<line number="%s" hits="0"/>\\n' "$i"
    i=$((i+1))
  done
  printf '</lines></class></classes></package></packages></coverage>\\n'
} > "$out/cobertura.xml"
: > "$out/index.html"
echo "781 passed; 7 skipped; 0 failed."
"""


class TestUnitCoverageLane(LaneCase):
    def test_unit_lane_measures_no_live_daemon_binary(self) -> None:
        self.install_stubs(SPARSE_REPORT)
        result = self.run_target("test-coverage-zig")
        self.assertEqual(result.returncode, 0, self.output(result))
        measured = self.binaries_measured()
        self.assertNotIn("zig-out/bin/agentsfleetd-integration-tests", measured)
        self.assertEqual(len(measured), 7, measured)
        self.assertIn("zig-out/bin/agentsfleetd-tests", measured)

    def test_unit_lane_does_not_grade_the_merged_floor(self) -> None:
        # It can see seven components of nine. A floor over those is a floor
        # over a different codebase, so it records what it measured and names
        # who grades it.
        self.install_stubs(SPARSE_REPORT)
        result = self.run_target("test-coverage-zig")
        self.assertEqual(result.returncode, 0, self.output(result))
        self.assertFalse((self.temp / "zig-coverage.txt").exists())
        self.assertIn("make test-coverage-grade", self.output(result))
        self.assertTrue((self.temp / "evidence" / "unit.json").exists())

    def test_a_failing_component_is_named_with_its_exit_status(self) -> None:
        # The attribution lives in scripts/check-kcov-components.sh now; this
        # proves it executes, not merely that its text survives.
        self.install_stubs(
            """\
            #!/bin/sh
            for arg in "$@"; do case "$arg" in --*) ;; *) out=$arg; break;; esac; done
            mkdir -p "$out"
            printf '<coverage/>\\n' > "$out/cobertura.xml"
            case "$out" in */deadline) echo "FAIL (TestExpectedEqual)"; exit 7;; esac
            echo "781 passed; 7 skipped; 0 failed."
            """,
        )
        result = self.run_target("test-coverage-zig")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("component deadline exited 7", self.output(result))

    def test_missing_component_report_fails(self) -> None:
        self.install_stubs(
            """\
            #!/bin/sh
            for arg in "$@"; do case "$arg" in --*) ;; *) out=$arg; break;; esac; done
            case "$out" in */runner) exit 0;; esac
            mkdir -p "$out"
            printf '<coverage line-rate="0.50"/>\\n' > "$out/cobertura.xml"
            echo "781 passed; 7 skipped; 0 failed."
            """,
        )
        result = self.run_target("test-coverage-zig")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("component runner produced no Cobertura report", self.output(result))

    def test_missing_kcov_names_install_hint(self) -> None:
        env = os.environ.copy()
        env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
        result = subprocess.run(
            ["make", "test-coverage-zig"],
            cwd=ROOT, env=env, text=True, capture_output=True, check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("install: brew install kcov", self.output(result))


class TestIntegrationLane(LaneCase):
    def test_the_live_daemon_binary_runs_once_unfiltered_and_once_filtered(self) -> None:
        self.install_stubs(SPARSE_REPORT)
        result = self.run_target("test-integration")
        self.assertEqual(result.returncode, 0, self.output(result))
        # Two executions of one binary, not two runs of one suite: the filtered
        # rebuild runs the boot-to-drain proof that the unfiltered run skips.
        self.assertEqual(
            self.binaries_measured(),
            ["zig-out/bin/agentsfleetd-integration-tests"] * 2,
        )
        self.assertIn("component=integration", self.output(result))
        self.assertIn("component=lifecycle", self.output(result))

    def test_a_suite_that_never_ran_fails_the_lane(self) -> None:
        self.install_stubs(SPARSE_REPORT.replace("781 passed", "0 passed"))
        result = self.run_target("test-integration")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no passing tests", self.output(result))

    def test_a_failing_suite_fails_the_lane(self) -> None:
        self.install_stubs(SPARSE_REPORT.replace("0 failed", "3 failed"))
        result = self.run_target("test-integration")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("3 failing test(s)", self.output(result))

    def test_a_skipped_lifecycle_proof_fails_the_lane(self) -> None:
        # The boot-to-drain proof skips itself without the datastores or the
        # isolation variable, and kcov still writes a perfectly valid report for
        # the process that started and stopped. Without this check the lane
        # grades that as `cmd/serve.zig` being genuinely uncovered, which is the
        # same number an honest regression produces.
        self.install_stubs(SPARSE_REPORT, lifecycle_ran=False)
        result = self.run_target("test-integration")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("lifecycle test did not run", self.output(result))

    def test_absent_unit_evidence_reports_the_ungraded_floor_without_failing(self) -> None:
        # Producing unit evidence was never this lane's job, so its absence is
        # reported, not punished. The integration verdict still stands.
        self.install_stubs(SPARSE_REPORT)
        result = self.run_target("test-integration")
        self.assertEqual(result.returncode, 0, self.output(result))
        self.assertIn("merged coverage floor not graded", self.output(result))
        self.assertIn("make test-unit-all", self.output(result))

    def test_a_narrowed_run_records_filtered_evidence_and_grades_nothing(self) -> None:
        self.install_stubs(SPARSE_REPORT)
        result = self.run_target("test-integration", "TEST_FILTER=integration(model_library)")
        self.assertEqual(result.returncode, 0, self.output(result))
        self.assertIn("TEST_FILTER narrowed this run", self.output(result))
        manifest = json.loads((self.temp / "evidence" / "integration.json").read_text())
        self.assertTrue(manifest["filtered"])


# The stub report is one file of ten lines, so every denominator assertion but
# the rate would fail on shape alone. Relaxing them is what leaves the rate as
# the only thing a floor test is testing; `test_below_floor_fails` then moves the
# one number it is about. Both lanes get the identical overrides, because the
# graph digest is taken over exactly these and evidence recorded under one graph
# must not validate under another.
STUB_SHAPED_GRAPH = (
    "ZIG_COVERAGE_MIN_FILES=0",
    "ZIG_COVERAGE_MIN_MEASURED_LINES=0",
    "ZIG_COVERAGE_FOLDER_FLOORS=",
    "ZIG_COVERAGE_FOLDER_TARGETS=",
    "ZIG_COVERAGE_REQUIRED_ROOTS=",
)


class TestMergedGrade(LaneCase):
    def run_sequence(self, minimum: str) -> subprocess.CompletedProcess[str]:
        self.install_stubs(SPARSE_REPORT)
        overrides = (f"ZIG_COVERAGE_MIN_PCT={minimum}", *STUB_SHAPED_GRAPH)
        unit = self.run_target("test-coverage-zig", *overrides)
        self.assertEqual(unit.returncode, 0, self.output(unit))
        return self.run_target("test-integration", *overrides)

    def test_the_canonical_sequence_grades_the_union(self) -> None:
        result = self.run_sequence(minimum="5")
        self.assertEqual(result.returncode, 0, self.output(result))
        self.assertTrue((self.temp / "zig-coverage.txt").exists())
        # Nine components across two lanes, each measured exactly once.
        self.assertEqual(len(self.binaries_measured()), 9)

    def test_below_floor_fails(self) -> None:
        # The report carries per-line hits, not a `line-rate` summary attribute.
        # The gate counts lines now precisely because trusting that attribute is
        # how a 24-file report read as 93.70% of the codebase.
        result = self.run_sequence(minimum="60")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("below threshold", self.output(result))

    def test_unit_evidence_from_another_build_fails_the_grade(self) -> None:
        self.install_stubs(SPARSE_REPORT)
        overrides = ("ZIG_COVERAGE_MIN_PCT=5", *STUB_SHAPED_GRAPH)
        unit = self.run_target("test-coverage-zig", *overrides)
        self.assertEqual(unit.returncode, 0, self.output(unit))
        manifest_path = self.temp / "evidence" / "unit.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["source_digest"] = "recorded-against-another-commit"
        manifest_path.write_text(json.dumps(manifest))
        result = self.run_target("test-integration", *overrides)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("source_digest mismatch", self.output(result))


class TestWorkflowWiring(unittest.TestCase):
    """CI invokes the same owners, once, and grades fail-closed.

    Source-level assertions: a second `make test-integration` in another
    workflow would silently reintroduce the duplicate this split removed.
    """

    WORKFLOWS = ROOT / ".github" / "workflows"

    def read(self, name: str) -> str:
        return (self.WORKFLOWS / name).read_text(encoding="utf-8")

    def commands(self, target: str) -> dict[str, int]:
        counts: dict[str, int] = {}
        for path in sorted(self.WORKFLOWS.glob("*.yml")):
            found = sum(
                line.strip().endswith(target) or f"{target} " in line or f"{target}'" in line
                for line in path.read_text(encoding="utf-8").splitlines()
                if not line.strip().startswith("#") and f"make {target}" in line
            )
            if found:
                counts[path.name] = found
        return counts

    def test_each_coverage_owner_is_invoked_once_in_one_workflow(self) -> None:
        # All three in the same workflow file, because artifact storage is
        # run-scoped: a grader anywhere else would poll for another run by
        # commit and defend cancellation and rerun races that have no local
        # equivalent.
        for target in ("test-coverage-zig", "test-integration", "test-coverage-grade"):
            with self.subTest(target=target):
                self.assertEqual(self.commands(target), {"test-integration.yml": 1})

    def test_the_grade_needs_both_producers_and_their_artifacts(self) -> None:
        body = self.read("test-integration.yml")
        self.assertIn("needs: [test-coverage-zig, test-integration-suite]", body)
        # Downloading both is what makes the needs entry more than ordering: a
        # grade over whichever artifact happened to arrive is the stale-evidence
        # hole the manifests exist to close.
        for artifact in ("zig-coverage-unit", "zig-coverage-integration"):
            self.assertEqual(body.count(f"name: {artifact}"), 2, artifact)

    def test_the_required_aggregate_cannot_go_green_around_the_grade(self) -> None:
        body = self.read("test-integration.yml")
        self.assertIn(
            "needs: [test-coverage-zig, test-integration-suite, zig-coverage-grade, test-integration-kernel]",
            body,
        )

    def test_the_unit_workflow_runs_no_zig_coverage(self) -> None:
        # `test.yml` kept the unit and package lanes; its coverage job moved
        # here whole. A surviving reference would mean two workflows racing to
        # write one summary file.
        self.assertNotIn("test-coverage-zig", self.read("test.yml"))


if __name__ == "__main__":
    unittest.main()
