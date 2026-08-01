"""Lane concurrency and local-cost gates.

Each assertion here guards a stretch of the test lanes that used to run one
thing at a time while the machine sat idle, or a local cost the lanes imposed
and never reclaimed. They are structural: the wall-clock win is only real if
the work is actually dispatched concurrently and its verdict is still
aggregated, so both halves are checked.
"""

import os
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MEMLEAK_LANE = ROOT / "scripts" / "run-zig-memleak-lane.sh"


def write_executable(path: Path, body: str) -> None:
    path.write_text(textwrap.dedent(body), encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


class TestCoverageConcurrency(unittest.TestCase):
    def setUp(self) -> None:
        self.makefile = (ROOT / "make/test-unit.mk").read_text(encoding="utf-8")

    def test_coverage_components_run_concurrently(self) -> None:
        # Backgrounded with a `wait` barrier before the merge, rather than a
        # serial `for` that runs kcov in the foreground.
        self.assertIn("wait; \\", self.makefile)
        self.assertNotIn(
            'kcov --clean --include-pattern="$(CURDIR)/src" "$$output" "zig-out/bin/$$binary" >"$$log"',
            self.makefile,
            "the serial foreground kcov invocation is still present",
        )

    def test_coverage_failure_is_still_attributed_and_fatal(self) -> None:
        # Concurrency must not lose which component failed, nor the non-zero exit.
        self.assertIn('rc=$$(cat ".tmp/kcov-$$name.rc"', self.makefile)
        self.assertIn('echo "✗ Zig coverage component $$name exited $$rc"', self.makefile)
        self.assertIn('[ "$$failed" -eq 0 ] || exit 1', self.makefile)


class TestMemleakOverlap(unittest.TestCase):
    def setUp(self) -> None:
        self.makefile = (ROOT / "make/bench.mk").read_text(encoding="utf-8")

    def test_boot_drain_overlaps_component_lanes(self) -> None:
        # Infra bring-up is dispatched with the lanes, not after them.
        self.assertIn("_ensure-test-infra >\"$$lane_dir/infra.log\" 2>&1 & infra_pid=$$!", self.makefile)
        self.assertIn('wait "$$infra_pid"', self.makefile)
        # A failed bring-up must fail the gate rather than let boot-drain run
        # against datastores that never came up.
        self.assertIn("test infra failed to come up", self.makefile)

    def test_boot_drain_still_runs_after_the_lanes(self) -> None:
        lanes = self.makefile.index('wait "$$lib_pid"')
        boot_drain = self.makefile.index("$(MAKE) _memleak-boot-drain")
        self.assertLess(lanes, boot_drain, "boot-drain must still follow the component lanes")


class TestMemleakLaneBinaries(unittest.TestCase):
    """Drive the real lane script with stub tools, on a stub platform."""

    def run_lane(
        self, binaries: list[str], failing: str | None = None
    ) -> tuple[subprocess.CompletedProcess[str], list[tuple[str, str, float]]]:
        with tempfile.TemporaryDirectory() as raw:
            temp = Path(raw)
            tool_dir = temp / "bin"
            tool_dir.mkdir()
            out_dir = temp / "zig-out" / "bin"
            out_dir.mkdir(parents=True)
            trace = temp / "trace.tsv"
            write_executable(tool_dir / "zig", "#!/bin/sh\nexit 0\n")
            # `uname` is stubbed to an unknown platform so the lane takes its
            # allocator-only branch: no valgrind, no macOS `leaks`, same
            # concurrency path.
            write_executable(tool_dir / "uname", "#!/bin/sh\necho StubOS\n")
            for binary in binaries:
                code = 1 if binary == failing else 0
                # Each stub brackets its own run in the shared trace. Overlap is
                # then a property of the recorded intervals rather than of the
                # harness's wall clock, so a loaded machine cannot fake either
                # verdict.
                write_executable(
                    out_dir / binary,
                    f"""\
                    #!/bin/sh
                    printf 'start\\t{binary}\\t%s\\n' "$(date +%s)" >> '{trace}'
                    sleep 1
                    printf 'end\\t{binary}\\t%s\\n' "$(date +%s)" >> '{trace}'
                    exit {code}
                    """,
                )
            env = os.environ.copy()
            env["PATH"] = f"{tool_dir}:{env.get('PATH', '')}"
            env["ZIG_GLOBAL_CACHE_DIR"] = str(temp / "global")
            env["ZIG_LOCAL_CACHE_DIR"] = str(temp / "local")
            env["UNAME_BIN"] = str(tool_dir / "uname")
            result = subprocess.run(
                ["bash", str(MEMLEAK_LANE), "lib", "-", "test-lib-bin", "0", *binaries],
                cwd=temp,
                env=env,
                capture_output=True,
                text=True,
            )
            events = []
            if trace.exists():
                for line in trace.read_text(encoding="utf-8").splitlines():
                    kind, name, stamp = line.split("\t")
                    events.append((kind, name, float(stamp)))
            return result, events

    def test_lib_lane_gates_binaries_concurrently(self) -> None:
        # Asserted structurally, like every other gate in this file, rather than
        # by timing a real lane run. The wall-clock form measured `date +%s`
        # stamps around a one-second sleep, which cannot resolve overlap once
        # process spawn costs more than a second — and the pre-commit hook runs
        # this inside `make -j` across five targets plus `zig build test-auth`,
        # so it failed there while passing standalone. A gate that blocks a
        # commit on how busy the machine is measures the machine, not the lane.
        lane = MEMLEAK_LANE.read_text(encoding="utf-8")
        # Each binary is dispatched in the background...
        self.assertIn(
            'gate_one "$binary" > "$log_dir/$binary.log" 2>&1 &',
            lane,
            "the per-binary gate is no longer backgrounded — the lane is serial again",
        )
        # ...and every one of their verdicts is still collected. Backgrounding
        # without this would be faster and worthless: a leaking binary's failure
        # would never reach the lane's exit status.
        self.assertIn(
            'wait "${pids[index]}" || status=1',
            lane,
            "per-binary verdicts are no longer aggregated after the fan-out",
        )

    def test_a_failing_binary_fails_the_lane(self) -> None:
        result, _ = self.run_lane(["alpha", "beta", "gamma"], failing="beta")
        self.assertNotEqual(result.returncode, 0, "a leaking binary must fail the lane")

    def test_output_is_grouped_per_binary(self) -> None:
        result, _ = self.run_lane(["alpha", "beta"])
        # Concurrent Valgrind reports interleaved into one stream are unreadable;
        # the lane replays each binary's output as a block, in list order.
        self.assertLess(
            result.stdout.index("alpha"),
            result.stdout.index("beta"),
            "per-binary output must be replayed in list order",
        )


class TestLocalCost(unittest.TestCase):
    def test_clean_removes_configured_cache(self) -> None:
        dev = (ROOT / "make/dev.mk").read_text(encoding="utf-8")
        self.assertIn('rm -rf "$(ZIG_LOCAL_CACHE_DIR)"', dev)
        # An unset variable must never turn the clean into a worktree wipe.
        self.assertIn("if [ -n \"$(strip $(ZIG_LOCAL_CACHE_DIR))\" ]", dev)

    def test_integration_keep_state_opt_out(self) -> None:
        integration = (ROOT / "make/test-integration.mk").read_text(encoding="utf-8")
        self.assertIn(
            "TEST_STATE_DEP := $(if $(KEEP_TEST_STATE),_ensure-test-infra,_reset-test-db)",
            integration,
        )
        # All three public integration targets share the switch; none may keep a
        # hard-wired reset that the opt-out silently fails to cover.
        self.assertEqual(integration.count("$(TEST_STATE_DEP)  ##"), 3)
        self.assertNotIn(": _reset-test-db  ##", integration)


if __name__ == "__main__":
    unittest.main()
