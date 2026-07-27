"""Equivalence gates for the sharded Zig test runner.

The shard runner replaces the runner that detects `std.testing.allocator`
leaks. A leak gate that stops detecting leaks is strictly worse than a slow
one, because it reports green — so these tests exist to make that failure
impossible to ship, not to characterise the speedup.

Three properties, in order of how much they matter:

  1. A leak still fails. A binary with one deliberately leaking test must exit
     non-zero, unsharded and in whichever shard owns it.
  2. The partition is exact and disjoint. Across every shard of a given count,
     each registered test runs exactly once — no test dropped, none run twice.
  3. A malformed shard environment aborts. It is never silently treated as
     "run everything", which would turn a typo into N full-suite passes that
     prove nothing about the partition while looking merely slow.

Fixtures are compiled with `zig test --test-runner`, so these run against the
real runner without needing any lane wired to it first.
"""

import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "src" / "build" / "test_runner_shard.zig"
FANOUT = ROOT / "scripts" / "run-zig-shards.sh"

INDEX_ENV = "AGENTSFLEET_TEST_SHARD_INDEX"
COUNT_ENV = "AGENTSFLEET_TEST_SHARD_COUNT"
EXIT_BAD_SHARD_ENV = 2

# `<position> <name>...OK|SKIP|FAIL`, the line the runner emits per test.
EXECUTED_RE = re.compile(r"^(\d+) (.+?)\.\.\.(OK|SKIP|FAIL)", re.M)

CLEAN_FIXTURE = """
const std = @import("std");
test "alpha" { try std.testing.expect(true); }
test "beta" { return error.SkipZigTest; }
test "gamma" { try std.testing.expect(true); }
test "delta" { try std.testing.expect(true); }
test "epsilon" { try std.testing.expect(true); }
"""

LEAKING_FIXTURE = """
const std = @import("std");
test "alpha" { try std.testing.expect(true); }
test "beta" { return error.SkipZigTest; }
test "gamma" { try std.testing.expect(true); }
test "delta leaks" {
    const buf = try std.testing.allocator.alloc(u8, 32);
    _ = buf;
}
"""

FAILING_FIXTURE = """
const std = @import("std");
test "alpha" { try std.testing.expect(true); }
test "beta fails" { try std.testing.expect(false); }
"""


def compile_fixture(workdir: Path, name: str, source: str) -> Path:
    src = workdir / f"{name}.zig"
    src.write_text(source, encoding="utf-8")
    binary = workdir / name
    result = subprocess.run(
        [
            "zig", "test",
            "--test-runner", str(RUNNER),
            "--test-no-exec",
            f"-femit-bin={binary}",
            str(src),
        ],
        cwd=ROOT, capture_output=True, text=True,
        env={
            **os.environ,
            "ZIG_GLOBAL_CACHE_DIR": str(ROOT / ".tmp" / "zig-global-cache-shardtest"),
            "ZIG_LOCAL_CACHE_DIR": str(ROOT / ".tmp" / "zig-local-cache-shardtest"),
        },
    )
    if result.returncode != 0:
        raise AssertionError(f"fixture {name} failed to compile:\n{result.stderr}")
    return binary


class ShardRunnerCase(unittest.TestCase):
    workdir: Path
    tmp: tempfile.TemporaryDirectory

    @classmethod
    def setUpClass(cls) -> None:
        cls.tmp = tempfile.TemporaryDirectory()
        cls.workdir = Path(cls.tmp.name)
        cls.clean = compile_fixture(cls.workdir, "clean", CLEAN_FIXTURE)
        cls.leaking = compile_fixture(cls.workdir, "leaking", LEAKING_FIXTURE)
        cls.failing = compile_fixture(cls.workdir, "failing", FAILING_FIXTURE)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.tmp.cleanup()

    def run_shard(self, binary: Path, index=None, count=None) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.pop(INDEX_ENV, None)
        env.pop(COUNT_ENV, None)
        if index is not None:
            env[INDEX_ENV] = str(index)
        if count is not None:
            env[COUNT_ENV] = str(count)
        return subprocess.run([str(binary)], capture_output=True, text=True, env=env)

    def executed(self, result: subprocess.CompletedProcess[str]) -> list[int]:
        return [int(m.group(1)) for m in EXECUTED_RE.finditer(result.stdout + result.stderr)]


class TestLeakDetection(ShardRunnerCase):
    def test_shard_runner_fails_on_leak(self) -> None:
        result = self.run_shard(self.leaking)
        self.assertNotEqual(result.returncode, 0, "an unsharded leak must fail the run")
        self.assertIn("leaked", result.stdout + result.stderr)

    def test_leak_fails_the_shard_that_owns_it(self) -> None:
        codes = [self.run_shard(self.leaking, i, 2).returncode for i in range(2)]
        self.assertIn(0, codes, "the shard without the leak should pass")
        self.assertTrue(any(c != 0 for c in codes), "the shard owning the leak must fail")

    def test_a_failing_test_still_fails(self) -> None:
        result = self.run_shard(self.failing)
        self.assertNotEqual(result.returncode, 0)


class TestPartition(ShardRunnerCase):
    def test_shard_partition_is_exact_and_disjoint(self) -> None:
        serial = self.executed(self.run_shard(self.clean))
        self.assertEqual(len(serial), 5, "the fixture registers five tests")
        for count in (1, 2, 3, 4, 5):
            union: list[int] = []
            for index in range(count):
                union.extend(self.executed(self.run_shard(self.clean, index, count)))
            self.assertEqual(
                sorted(union), sorted(serial),
                f"count={count} did not reproduce the serial set exactly",
            )
            self.assertEqual(len(union), len(set(union)), f"count={count} ran a test twice")

    def test_unsharded_default_runs_everything(self) -> None:
        # A lane that has not been migrated must behave exactly as before.
        bare = self.executed(self.run_shard(self.clean))
        single = self.executed(self.run_shard(self.clean, 0, 1))
        self.assertEqual(bare, single)

    def test_more_shards_than_tests_is_harmless(self) -> None:
        union: list[int] = []
        for index in range(9):
            result = self.run_shard(self.clean, index, 9)
            self.assertEqual(result.returncode, 0, "an empty shard must still exit 0")
            union.extend(self.executed(result))
        self.assertEqual(len(union), 5)


class TestMalformedEnvironment(ShardRunnerCase):
    def test_non_numeric_count_aborts(self) -> None:
        result = self.run_shard(self.clean, 0, "four")
        self.assertEqual(result.returncode, EXIT_BAD_SHARD_ENV)
        self.assertEqual(self.executed(result), [], "a bad selector must run no tests")

    def test_index_beyond_count_aborts(self) -> None:
        result = self.run_shard(self.clean, 5, 2)
        self.assertEqual(result.returncode, EXIT_BAD_SHARD_ENV)

    def test_zero_count_aborts(self) -> None:
        result = self.run_shard(self.clean, 0, 0)
        self.assertEqual(result.returncode, EXIT_BAD_SHARD_ENV)

    def test_empty_selector_falls_back_to_single_shard(self) -> None:
        result = self.run_shard(self.clean, "", "")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(self.executed(result)), 5)


class TestFanOut(ShardRunnerCase):
    def fan_out(self, binary: Path, count: int) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", str(FANOUT), str(count), str(binary)],
            capture_output=True, text=True,
        )

    def test_fanout_preserves_shard_output(self) -> None:
        result = self.fan_out(self.leaking, 2)
        self.assertNotEqual(result.returncode, 0, "a leaking shard must fail the fan-out")
        # The failing shard is the only one anyone wants to read, so it leads.
        self.assertLess(
            result.stdout.index("FAILED"),
            result.stdout.index("ok ──"),
            "the failing shard's output must be replayed first",
        )

    def test_fanout_passes_a_clean_binary(self) -> None:
        result = self.fan_out(self.clean, 3)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("all 3 shards passed", result.stdout)

    def test_fanout_rejects_a_bad_count(self) -> None:
        result = self.fan_out(self.clean, 0)
        self.assertEqual(result.returncode, EXIT_BAD_SHARD_ENV)


if __name__ == "__main__":
    unittest.main()
