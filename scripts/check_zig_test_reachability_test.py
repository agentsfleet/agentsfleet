#!/usr/bin/env python3
"""Self-tests for check_zig_test_reachability.py.

The gate is only worth having if it fails on a dead file, stays quiet on a waived
one, and refuses to credit blocks that never compiled. Each test builds a fixture
tree and a synthetic registered-name set, so nothing here shells out to `zig build`.

Run: python3 scripts/check_zig_test_reachability_test.py
"""

import contextlib
import io
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_zig_test_reachability as checker  # noqa: E402

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QUALITY_MK = os.path.join(REPO_ROOT, "make", "quality.mk")
CHECKER_TARGET = "_lint_zig_test_reachability"

ROOT_DIR = "src/agentsfleetd"


class ReachabilityTestCase(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self._prev_root = checker.REPO_ROOT
        checker.REPO_ROOT = self._tmp.name
        self.addCleanup(self._restore)

    def _restore(self):
        checker.REPO_ROOT = self._prev_root
        self._tmp.cleanup()

    def write(self, rel_path, body):
        full = os.path.join(self._tmp.name, rel_path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as handle:
            handle.write(body)
        return rel_path

    def run_check(self, groups, candidates):
        err = io.StringIO()
        out = io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
            code = checker.run_check(groups, candidates)
        return code, err.getvalue() + out.getvalue()

    def run_count(self, groups, candidates):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            checker.run_count(groups, candidates)
        return out.getvalue()


class TestReachability(ReachabilityTestCase):
    def test_reachability_flags_unwired_fixture(self):
        dead = self.write(f"{ROOT_DIR}/orphan.zig", 'test "never runs" {}\n')
        code, output = self.run_check({ROOT_DIR: set()}, [dead])
        self.assertEqual(code, 1)
        self.assertIn(dead, output)

    def test_reachability_waiver_exempts(self):
        dead = self.write(
            f"{ROOT_DIR}/orphan.zig",
            '// no-test-root: fixture\ntest "never runs" {}\n',
        )
        code, output = self.run_check({ROOT_DIR: set()}, [dead])
        self.assertEqual(code, 0)
        self.assertNotIn("orphan.zig  (", output)

    def test_live_file_passes(self):
        live = self.write(f"{ROOT_DIR}/db/pool.zig", 'test "pools" {}\n')
        groups = {ROOT_DIR: {"db.pool.test.pools"}}
        self.assertTrue(checker.is_live(live, groups))
        self.assertEqual(self.run_check(groups, [live])[0], 0)

    def test_anonymous_test_block_counts_as_registered(self):
        live = self.write(f"{ROOT_DIR}/barrel.zig", 'test "named" {}\n')
        groups = {ROOT_DIR: {"barrel.test_0"}}
        self.assertTrue(checker.is_live(live, groups))

    def test_namespace_is_path_relative_to_its_root(self):
        deep = self.write(f"{ROOT_DIR}/a/b/c.zig", 'test "deep" {}\n')
        self.assertTrue(checker.is_live(deep, {ROOT_DIR: {"a.b.c.test.deep"}}))
        self.assertFalse(checker.is_live(deep, {ROOT_DIR: {"c.test.deep"}}))

    def test_file_live_via_a_second_root_is_not_dead(self):
        """src/agentsfleetd/auth/** registers in both the daemon and auth binaries."""
        path = self.write("src/agentsfleetd/auth/jwt.zig", 'test "verify" {}\n')
        groups = {ROOT_DIR: set(), "src/agentsfleetd/auth": {"jwt.test.verify"}}
        self.assertTrue(checker.is_live(path, groups))

    def test_duplicate_description_does_not_mask_a_dead_file(self):
        """A dead file sharing a test description with a live one stays dead.

        Matching on the description alone would report `dead.zig` as reachable
        because `live.zig` registered the same string.
        """
        live = self.write(f"{ROOT_DIR}/live.zig", 'test "same words" {}\n')
        dead = self.write(f"{ROOT_DIR}/dead.zig", 'test "same words" {}\n')
        groups = {ROOT_DIR: {"live.test.same words"}}
        self.assertTrue(checker.is_live(live, groups))
        self.assertFalse(checker.is_live(dead, groups))
        code, output = self.run_check(groups, [dead, live])
        self.assertEqual(code, 1)
        self.assertIn(dead, output)


class TestDepthCount(ReachabilityTestCase):
    def test_depth_counts_registered_only(self):
        live = self.write(
            f"{ROOT_DIR}/live.zig",
            'test "one" {}\ntest "integration: two" {}\n',
        )
        dead = self.write(f"{ROOT_DIR}/dead.zig", 'test "three" {}\n')
        groups = {ROOT_DIR: {"live.test.one"}}
        output = self.run_count(groups, [live, dead])
        self.assertIn("reachable_test_cases=2", output)
        self.assertIn("reachable_integration_cases=1", output)

    def test_count_never_exceeds_the_textual_block_count(self):
        live = self.write(f"{ROOT_DIR}/live.zig", 'test "a" {}\ntest "b" {}\n')
        dead = self.write(f"{ROOT_DIR}/dead.zig", 'test "c" {}\n')
        textual = sum(checker.count_blocks(p)[0] for p in (live, dead))
        output = self.run_count({ROOT_DIR: {"live.test.a"}}, [live, dead])
        reachable = int(output.split("reachable_test_cases=")[1].split("\n")[0])
        self.assertLess(reachable, textual)

    def test_depth_gate_red_on_unwire(self):
        """Un-wiring a reachable file drops the count and turns the gate red."""
        path = self.write(f"{ROOT_DIR}/wired.zig", 'test "x" {}\n')
        wired = {ROOT_DIR: {"wired.test.x"}}
        unwired = {ROOT_DIR: set()}

        before = self.run_count(wired, [path])
        after = self.run_count(unwired, [path])
        self.assertIn("reachable_test_cases=1", before)
        self.assertIn("reachable_test_cases=0", after)
        self.assertEqual(self.run_check(wired, [path])[0], 0)
        self.assertEqual(self.run_check(unwired, [path])[0], 1)


class TestMakeWiring(unittest.TestCase):
    """The gate is worthless if `make lint-zig` never calls it."""

    def setUp(self):
        with open(QUALITY_MK) as handle:
            self.quality_mk = handle.read()

    def test_reachability_wired_into_lint_zig(self):
        prereqs = next(
            line for line in self.quality_mk.splitlines() if line.startswith("lint-zig:")
        )
        self.assertIn(CHECKER_TARGET, prereqs)

    def test_reachability_target_invokes_the_checker(self):
        self.assertIn(f"{CHECKER_TARGET}:", self.quality_mk)
        self.assertIn("check_zig_test_reachability.py --check", self.quality_mk)

    def test_depth_gate_consumes_the_checker_count(self):
        self.assertIn("check_zig_test_reachability.py --count", self.quality_mk)
        self.assertNotIn("""find src -name '*.zig' -exec grep -hE '^test "'""", self.quality_mk)


if __name__ == "__main__":
    unittest.main(verbosity=2)
