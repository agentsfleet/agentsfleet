import os
from pathlib import Path
import stat
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
MEMLEAK_RUNNER = ROOT / "scripts" / "run-zig-memleak-lane.sh"


# The coverage lane greps the lifecycle component's log for this marker, because
# that test skips itself without live datastores and a skipped run still yields a
# valid report — of a process that started and stopped. These stubs stand in for
# the real binary, so by default they say what it says when it runs.
LIFECYCLE_RUN_MARKER = "SERVE_LIFECYCLE_BOOT_DRAIN_RAN"
LIFECYCLE_MARKER_ECHO = f'echo "{LIFECYCLE_RUN_MARKER}"\n'


def write_executable(path: Path, body: str) -> None:
    path.write_text(textwrap.dedent(body), encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


class TestLaneGraph(unittest.TestCase):
    def test_daemon_roots_are_disjoint(self) -> None:
        unit_root = (ROOT / "src/agentsfleetd/tests.zig").read_text()
        integration_root = (ROOT / "src/agentsfleetd/integration_tests.zig").read_text()
        build_graph = (ROOT / "src/build/daemon_tests.zig").read_text()
        self.assertNotIn("_integration_test.zig", unit_root)
        self.assertIn("_integration_test.zig", integration_root)
        self.assertNotIn('@import("main.zig")', integration_root)
        self.assertIn('S_INTEGRATION_FILE_FILTER = "_integration_test"', build_graph)
        self.assertIn('S_INTEGRATION_NAME_FILTER = "integration:"', build_graph)
        self.assertIn('@import("cron/fire_queue_integration_test.zig")', integration_root)

    def test_public_integration_targets_run_integration_graph(self) -> None:
        makefile = (ROOT / "make/test-integration.mk").read_text()
        # The narrow selectors still run the graph through the build system. The
        # full lane builds the binary instead, because kcov needs one to drive,
        # and that instrumented run is now the only one there is.
        self.assertEqual(makefile.count("zig build test-integration $(ZIG_TEST_FILTER_ARG)"), 2)
        self.assertIn("zig build install test-integration-bin", makefile)
        self.assertNotIn("zig build test\n", makefile)
        self.assertIn("test-integration: $(TEST_STATE_DEP)", makefile)

    def test_only_the_integration_lane_measures_the_live_daemon_binary(self) -> None:
        # The duplicate this removed: the unit coverage lane ran this binary
        # under kcov and the integration lane ran it again bare, so one full
        # verification executed ~2000 live-service tests twice.
        #
        # `bench.mk` names it too and is not a second owner: the memleak lane
        # drives one filtered test under valgrind for a leak proof, a different
        # instrument answering a different question, contributing nothing to the
        # coverage union.
        owners = set()
        for path in (ROOT / "make").glob("*.mk"):
            body = path.read_text()
            if "agentsfleetd-integration-tests" in body or "ZIG_INTEGRATION_TEST_BIN" in body:
                owners.add(path.name)
        self.assertEqual(owners, {"test.mk", "test-integration.mk", "bench.mk"})
        self.assertNotIn(
            "agentsfleetd-integration-tests", (ROOT / "make/test-unit.mk").read_text()
        )

    def test_the_merged_grade_has_one_owner(self) -> None:
        # Neither producer can see the union — the unit lane no longer runs the
        # live components and the integration lane never runs the unit ones — so
        # a floor graded from inside either would be a floor over half a
        # codebase. The published summary file CI reads has the same one owner.
        graders = {
            path.name
            for path in (ROOT / "make").glob("*.mk")
            if "check_zig_coverage.py" in path.read_text()
        }
        self.assertEqual(graders, {"test.mk"})
        self.assertIn("test-coverage-grade:", (ROOT / "make/test.mk").read_text())

    def test_integration_reset_is_the_default_dependency(self) -> None:
        # The reset dependency became a variable so an iterative local loop can
        # opt out of it. The gate default must still be the full reset, so this
        # asserts the resolved graph rather than the literal prerequisite: the
        # indirection is only safe if it resolves the way the old literal read.
        plan = subprocess.run(
            ["make", "-n", "test-integration"],
            cwd=ROOT, capture_output=True, text=True,
        ).stdout
        self.assertIn("teardown.sql", plan, "the default integration lane must still reset the database")

        opted_out = subprocess.run(
            ["make", "-n", "test-integration", "KEEP_TEST_STATE=1"],
            cwd=ROOT, capture_output=True, text=True,
        ).stdout
        self.assertNotIn("teardown.sql", opted_out, "KEEP_TEST_STATE=1 must skip the reset")

    def test_private_memleak_helper_has_three_callers(self) -> None:
        makefile = (ROOT / "make/bench.mk").read_text()
        self.assertEqual(makefile.count("$(MAKE) _memleak-lane"), 3)
        self.assertIn("& daemon_pid=$$!", makefile)
        self.assertIn("& runner_pid=$$!", makefile)
        self.assertIn("& lib_pid=$$!", makefile)
        self.assertIn('wait "$$daemon_pid"', makefile)
        self.assertIn('wait "$$runner_pid"', makefile)
        self.assertIn('wait "$$lib_pid"', makefile)

    def test_boot_drain_uses_integration_binary(self) -> None:
        makefile = (ROOT / "make/bench.mk").read_text()
        self.assertIn("zig build test-integration-bin", makefile)
        self.assertIn("zig-out/bin/agentsfleetd-integration-tests", makefile)
        self.assertNotIn("zig build test-bin $$opt -Dtest-filter", makefile)


class TestMemleakLane(unittest.TestCase):
    def run_lane(
        self,
        platform: str,
        leaks_supported: str = "0",
        valgrind_exit: str = "0",
    ) -> tuple[subprocess.CompletedProcess[str], str]:
        with tempfile.TemporaryDirectory() as raw:
            temp = Path(raw)
            tool_dir = temp / "bin"
            tool_dir.mkdir()
            log = temp / "calls.log"
            write_executable(
                tool_dir / "uname",
                f"#!/bin/sh\nprintf '%s\\n' {platform}\n",
            )
            write_executable(
                tool_dir / "zig",
                """\
                #!/bin/sh
                printf 'zig global=%s local=%s\\n' "$ZIG_GLOBAL_CACHE_DIR" "$ZIG_LOCAL_CACHE_DIR" >> "$CALL_LOG"
                exit 0
                """,
            )
            write_executable(
                tool_dir / "leaks",
                "#!/bin/sh\nprintf 'leaks\\n' >> \"$CALL_LOG\"\nexit 0\n",
            )
            write_executable(
                tool_dir / "valgrind",
                f"#!/bin/sh\nprintf 'valgrind\\n' >> \"$CALL_LOG\"\nexit {valgrind_exit}\n",
            )
            binary = temp / "zig-out/bin/sample-tests"
            binary.parent.mkdir(parents=True)
            write_executable(
                binary,
                "#!/bin/sh\nprintf 'binary\\n' >> \"$CALL_LOG\"\nexit 0\n",
            )
            env = os.environ.copy()
            env.update(
                {
                    "PATH": f"{tool_dir}:/usr/bin:/bin",
                    "UNAME_BIN": str(tool_dir / "uname"),
                    "CALL_LOG": str(log),
                    "ZIG_GLOBAL_CACHE_DIR": str(temp / "global"),
                    "ZIG_LOCAL_CACHE_DIR": str(temp / "local"),
                    "MACOS_LEAKS_SUPPORTED": leaks_supported,
                }
            )
            result = subprocess.run(
                [str(MEMLEAK_RUNNER), "sample", "-", "test-bin", "1", "sample-tests"],
                cwd=temp,
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )
            return result, log.read_text(encoding="utf-8")

    def test_macos_failed_preflight_skips_advisory_rerun(self) -> None:
        result, calls = self.run_lane("Darwin")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls.count("binary"), 1)
        self.assertNotIn("leaks", calls)
        self.assertIn("global=", calls)

    def test_macos_supported_inspection_runs_advisory(self) -> None:
        result, calls = self.run_lane("Darwin", leaks_supported="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("leaks", calls)

    def test_linux_valgrind_failure_propagates(self) -> None:
        result, calls = self.run_lane("Linux", valgrind_exit="9")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("valgrind", calls)


if __name__ == "__main__":
    unittest.main()
