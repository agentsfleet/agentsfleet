import os
from pathlib import Path
import stat
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
MEMLEAK_RUNNER = ROOT / "scripts" / "run-zig-memleak-lane.sh"


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
        self.assertEqual(makefile.count("zig build test-integration"), 3)
        self.assertNotIn("zig build test\n", makefile)
        self.assertIn("test-integration: $(TEST_STATE_DEP)", makefile)

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


class TestCoverageLane(unittest.TestCase):
    def run_coverage(self, kcov_body: str, minimum: str = "60") -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as raw:
            temp = Path(raw)
            tool_dir = temp / "bin"
            tool_dir.mkdir()
            write_executable(tool_dir / "zig", "#!/bin/sh\nexit 0\n")
            write_executable(tool_dir / "kcov", kcov_body)
            # The lane now measures the integration binary too, which needs a
            # live Postgres and Redis. These tests are about the gate's
            # arithmetic and its failure messages, not about provisioning, so
            # they declare the datastores already supplied — the same escape
            # hatch continuous integration uses when it boots them itself.
            cert = temp / "redis-ca.crt"
            cert.write_text("stub cert: TEST_INFRA=provided only checks it is non-empty\n")
            env = os.environ.copy()
            env["PATH"] = f"{tool_dir}:/usr/bin:/bin:/usr/sbin:/sbin"
            return subprocess.run(
                [
                    "make",
                    "test-coverage-zig",
                    f"ZIG_COVERAGE_DIR={temp / 'coverage'}",
                    f"ZIG_GLOBAL_CACHE_DIR={temp / 'global'}",
                    f"ZIG_LOCAL_CACHE_DIR={temp / 'local'}",
                    f"ZIG_COVERAGE_MIN_PCT={minimum}",
                    "TEST_INFRA=provided",
                    "KEEP_TEST_STATE=1",
                    f"TEST_REDIS_TLS_CA_CERT={cert}",
                ],
                cwd=ROOT,
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )

    def test_below_floor_fails(self) -> None:
        # The report carries per-line hits, not a `line-rate` summary attribute.
        # The gate counts lines now precisely because trusting that attribute is
        # how a 24-file report read as 93.70% of the codebase.
        result = self.run_coverage(
            """\
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
            """,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("below threshold", result.stdout + result.stderr)

    def test_missing_component_report_fails(self) -> None:
        result = self.run_coverage(
            """\
            #!/bin/sh
            for arg in "$@"; do case "$arg" in --*) ;; *) out=$arg; break;; esac; done
            case "$out" in */runner) exit 0;; esac
            mkdir -p "$out"
            printf '<coverage line-rate="0.50"/>\\n' > "$out/cobertura.xml"
            echo "781 passed; 7 skipped; 0 failed."
            """,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("component runner produced no Cobertura report", result.stdout + result.stderr)

    def test_missing_kcov_names_install_hint(self) -> None:
        env = os.environ.copy()
        env["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
        result = subprocess.run(
            ["make", "test-coverage-zig"],
            cwd=ROOT,
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("install: brew install kcov", result.stdout + result.stderr)


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
