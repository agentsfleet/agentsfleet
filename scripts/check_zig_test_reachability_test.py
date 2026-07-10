#!/usr/bin/env python3
"""Self-tests for check_zig_test_reachability.py — reachability + depth counting.

The gate is only worth having if it fails on a dead file, refuses to launder a
waiver into "reachable", and never credits a block that did not compile.

Subprocess parsing and CLI dispatch live in check_zig_test_reachability_cli_test.py.
Run both: python3 -m unittest discover -s scripts -t scripts -p 'check_zig_test_reachability*_test.py'
"""

import contextlib
import io
import os
import unittest

from reachability_test_support import (  # noqa: E402
    CHECKER_TARGET,
    QUALITY_MK,
    REACHABILITY_MK,
    REPO_ROOT,
    ROOT_DIR,
    ReachabilityTestCase,
    checker,
)


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

    def test_empty_waiver_reason_does_not_waive(self):
        """`// no-test-root:` with nothing after it is a silent opt-out, so it fails."""
        path = self.write(f"{ROOT_DIR}/orphan.zig", '// no-test-root:\ntest "x" {}\n')
        self.assertIsNone(checker.waiver_reason(path))
        self.assertEqual(self.run_check({ROOT_DIR: set()}, [path])[0], 1)

    def test_ambiguous_dotted_filename_is_rejected(self):
        """`a/b.c.zig` and `a/b/c.zig` both namespace to `a.b.c`; refuse the collision."""
        dotted = self.write(f"{ROOT_DIR}/db.pool.zig", 'test "x" {}\n')
        self.assertTrue(checker.has_ambiguous_name(dotted))
        self.assertFalse(checker.has_ambiguous_name(f"{ROOT_DIR}/db/pool.zig"))
        code, output = self.run_check({ROOT_DIR: {"db.pool.test.x"}}, [dotted])
        self.assertEqual(code, 1)
        self.assertIn("ambiguous test namespace", output)

    def test_waived_file_is_not_counted_as_reachable(self):
        """A waived file is excluded from the reachable count and named with its
        reason — a waiver nobody reads is how a block goes dark to begin with."""
        waived = self.write(
            f"{ROOT_DIR}/orphan.zig",
            '// no-test-root: only compiles under -Dfuzz\ntest "x" {}\n',
        )
        live = self.write(f"{ROOT_DIR}/live.zig", 'test "y" {}\n')
        code, output = self.run_check({ROOT_DIR: {"live.test.y"}}, [waived, live])
        self.assertEqual(code, 0)
        self.assertIn("1 file(s) reachable, 1 waived", output)
        self.assertIn("only compiles under -Dfuzz", output)
        self.assertIn(waived, output)

    def test_stale_waiver_on_a_live_file_is_reported(self):
        path = self.write(
            f"{ROOT_DIR}/live.zig",
            '// no-test-root: left over from an old refactor\ntest "y" {}\n',
        )
        code, output = self.run_check({ROOT_DIR: {"live.test.y"}}, [path])
        self.assertEqual(code, 0)
        self.assertIn("stale waiver", output)

    def test_file_under_no_test_root_is_dead(self):
        """`src/build/**` sits under no binary's root, so a test there can never run."""
        stray = self.write("src/build/helper.zig", 'test "orphan" {}\n')
        groups = {ROOT_DIR: {"db.pool.test.pools"}}
        self.assertFalse(checker.is_live(stray, groups))
        self.assertEqual(self.run_check(groups, [stray])[0], 1)

    def test_waiver_requires_the_exact_marker(self):
        """A bare `// no-test-root` without the colon is not a waiver."""
        near_miss = self.write(
            f"{ROOT_DIR}/orphan.zig",
            '// no-test-root\ntest "never runs" {}\n',
        )
        self.assertFalse(checker.is_waived(near_miss))
        self.assertEqual(self.run_check({ROOT_DIR: set()}, [near_miss])[0], 1)

    def test_only_column_zero_test_lines_count(self):
        """Preserves the historical `^test "` semantics: an indented block is not a
        top-level test declaration, and a `test "` inside a doc-comment is not code."""
        path = self.write(
            f"{ROOT_DIR}/x.zig",
            'test "real" {}\n    test "indented" {}\n// test "commented" {}\n',
        )
        self.assertEqual(checker.count_blocks(path), (1, 0))

    def test_registered_names_exits_when_a_lane_prints_nothing(self):
        """An empty listing means the lane is unwired; treating it as 'no tests exist'
        would silently mark the entire tree dead, or (worse) pass a --count of 0."""
        class FakeProc:
            returncode = 0
            stdout = ""
            stderr = ""

        original = checker.subprocess.run
        checker.subprocess.run = lambda *a, **k: FakeProc()
        try:
            with contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as caught:
                    checker.registered_names()
            self.assertEqual(caught.exception.code, 1)
        finally:
            checker.subprocess.run = original

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
        with open(REACHABILITY_MK) as handle:
            self.reachability_mk = handle.read()

    def test_reachability_wired_into_lint_zig(self):
        prereqs = next(
            line for line in self.quality_mk.splitlines() if line.startswith("lint-zig:")
        )
        self.assertIn(CHECKER_TARGET, prereqs)

    def test_reachability_mk_is_included_by_the_makefile(self):
        """The recipes moved out of quality.mk under RULE FLL; an unincluded .mk
        would make every reachability assertion below vacuously true."""
        with open(os.path.join(REPO_ROOT, "Makefile")) as handle:
            self.assertIn("include make/reachability.mk", handle.read())

    def test_reachability_target_invokes_the_checker(self):
        self.assertIn(f"{CHECKER_TARGET}:", self.reachability_mk)
        self.assertIn("check_zig_test_reachability.py --check", self.reachability_mk)

    def test_depth_gate_consumes_the_reachability_counts(self):
        """One listing, not two: the depth gate reads what --check already produced."""
        self.assertIn("--check --counts-out $(REACHABLE_COUNTS)", self.reachability_mk)
        self.assertIn("_lint_zig_test_depth: _lint_zig_test_reachability", self.reachability_mk)
        self.assertIn("counts=$$(cat $(REACHABLE_COUNTS))", self.reachability_mk)
        self.assertNotIn(
            """find src -name '*.zig' -exec grep -hE '^test "'""",
            self.reachability_mk + self.quality_mk,
            msg="depth gate must not fall back to the textual scan",
        )

    def test_depth_gate_fails_closed_on_a_bad_count(self):
        """The recipe must `set -eu` and reject a non-numeric total: the old form
        printed success with a blank count when the checker errored."""
        recipe = self.reachability_mk.split("_lint_zig_test_depth:")[1]
        self.assertIn("set -eu", recipe)
        self.assertIn("*[!0-9]*", recipe)

    def test_git_dependent_gates_fail_closed_on_an_empty_scan(self):
        """The CI container runs as root over a runner-owned checkout, so plain git
        exits 128 ("dubious ownership"). A `for f in $(git ...)` loop then iterates
        nothing and the gate prints ✓ having inspected zero files — which is what
        _zig_line_limit_check did in CI. Every git call takes -c safe.directory, and
        every scan asserts it matched something."""
        for call in ("ls-files '*.zig'", "grep -hoE 'playbooks/"):
            self.assertNotIn(
                f"git {call}",
                self.quality_mk,
                msg=f"`git {call}` must pass -c safe.directory (CI container)",
            )
        line_limit = self.quality_mk.split("_zig_line_limit_check:")[1].split("\n\n")[0]
        self.assertIn("git -c safe.directory='*' ls-files", line_limit)
        self.assertIn("listed zero Zig files", line_limit)

        playbooks = self.quality_mk.split("check-playbooks:")[1]
        self.assertIn("git -c safe.directory='*' grep", playbooks)
        self.assertIn("matched nothing", playbooks)


if __name__ == "__main__":
    unittest.main(verbosity=2)
