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


class FakeProc:
    def __init__(self, stdout="", returncode=0, stderr=""):
        self.stdout = stdout
        self.returncode = returncode
        self.stderr = stderr


class SubprocessFakeCase(ReachabilityTestCase):
    """Replaces subprocess.run with a scripted queue of results."""

    def fake_subprocess(self, *results):
        queue = list(results)
        original = checker.subprocess.run
        checker.subprocess.run = lambda *a, **k: queue.pop(0)
        self.addCleanup(lambda: setattr(checker.subprocess, "run", original))


class TestRegisteredNames(SubprocessFakeCase):
    def test_each_test_line_carries_its_own_root(self):
        self.fake_subprocess(
            FakeProc("ROOT\tsrc/agentsfleetd\nTEST\tsrc/agentsfleetd\tdb.pool.test.a\n"),
            FakeProc("ROOT\tsrc/runner\nTEST\tsrc/runner\tengine.test.b\n"),
        )
        groups = checker.registered_names()
        self.assertEqual(groups["src/agentsfleetd"], {"db.pool.test.a"})
        self.assertEqual(groups["src/runner"], {"engine.test.b"})

    def test_interleaved_lane_output_is_attributed_correctly(self):
        """`zig build` runs lanes on a thread pool. If stdout ever interleaves, a
        root-carrying TEST line still lands in the right group; a positional parser
        would misfile it and silently flip a file's live/dead verdict."""
        self.fake_subprocess(
            FakeProc(
                "ROOT\tsrc/lib\n"
                "TEST\tsrc/runner\tengine.test.b\n"   # runner's line lands mid-lib block
                "ROOT\tsrc/runner\n"
                "TEST\tsrc/lib\tclock.test.a\n"
            ),
            FakeProc("ROOT\tsrc/agentsfleetd\n"),
        )
        groups = checker.registered_names()
        self.assertEqual(groups["src/lib"], {"clock.test.a"})
        self.assertEqual(groups["src/runner"], {"engine.test.b"})

    def test_two_binaries_sharing_a_root_accumulate(self):
        """The runner's unit and integration lanes both root at src/runner."""
        self.fake_subprocess(
            FakeProc("ROOT\tsrc/runner\nTEST\tsrc/runner\ta.test.one\n"),
            FakeProc("ROOT\tsrc/runner\nTEST\tsrc/runner\tb.test.two\n"),
        )
        self.assertEqual(checker.registered_names()["src/runner"], {"a.test.one", "b.test.two"})

    def test_a_lane_registering_nothing_still_records_its_root(self):
        """Otherwise a silently-empty binary looks like 'no candidates under it'."""
        self.fake_subprocess(
            FakeProc("ROOT\tsrc/lib\n"),
            FakeProc("ROOT\tsrc/runner\nTEST\tsrc/runner\tz.test.z\n"),
        )
        groups = checker.registered_names()
        self.assertIn("src/lib", groups)
        self.assertEqual(groups["src/lib"], set())

    def test_exits_when_a_lane_fails_to_build(self):
        self.fake_subprocess(FakeProc(returncode=1, stderr="compile error"))
        with contextlib.redirect_stderr(io.StringIO()) as err:
            with self.assertRaises(SystemExit) as caught:
                checker.registered_names()
        self.assertEqual(caught.exception.code, 1)
        self.assertIn("compile error", err.getvalue())


class TestCandidateFiles(SubprocessFakeCase):
    def test_selects_only_zig_files_carrying_a_test_block(self):
        self.write(f"{ROOT_DIR}/with_test.zig", 'test "a" {}\n')
        self.write(f"{ROOT_DIR}/no_test.zig", "pub fn f() void {}\n")
        self.write(f"{ROOT_DIR}/notes.md", 'test "not zig" {}\n')
        listing = f"{ROOT_DIR}/with_test.zig\n{ROOT_DIR}/no_test.zig\n{ROOT_DIR}/notes.md\n"
        self.fake_subprocess(FakeProc(listing))
        self.assertEqual(checker.candidate_files(), [f"{ROOT_DIR}/with_test.zig"])


class TestMainDispatch(ReachabilityTestCase):
    def setUp(self):
        super().setUp()
        original_argv = sys.argv
        self.addCleanup(lambda: setattr(sys, "argv", original_argv))

    def _stub(self, groups, candidates):
        for name, value in (("registered_names", groups), ("candidate_files", candidates)):
            original = getattr(checker, name)
            setattr(checker, name, lambda v=value: v)
            self.addCleanup(lambda n=name, o=original: setattr(checker, n, o))

    def test_check_flag_returns_nonzero_on_a_dead_file(self):
        dead = self.write(f"{ROOT_DIR}/orphan.zig", 'test "x" {}\n')
        self._stub({ROOT_DIR: set()}, [dead])
        sys.argv = ["prog", "--check"]
        with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(checker.main(), 1)

    def test_check_writes_counts_out_so_the_depth_gate_needs_no_second_listing(self):
        live = self.write(f"{ROOT_DIR}/live.zig", 'test "x" {}\ntest "integration: y" {}\n')
        self._stub({ROOT_DIR: {"live.test.x"}}, [live])
        out_path = os.path.join(self._tmp.name, "counts.txt")
        sys.argv = ["prog", "--check", "--counts-out", out_path]
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(checker.main(), 0)
        with open(out_path) as handle:
            written = handle.read()
        self.assertIn("reachable_test_cases=2", written)
        self.assertIn("reachable_integration_cases=1", written)

    def test_check_does_not_write_counts_when_a_dead_file_fails_the_gate(self):
        """A red gate must not leave a counts file a later step could consume."""
        dead = self.write(f"{ROOT_DIR}/orphan.zig", 'test "x" {}\n')
        self._stub({ROOT_DIR: set()}, [dead])
        out_path = os.path.join(self._tmp.name, "counts.txt")
        sys.argv = ["prog", "--check", "--counts-out", out_path]
        with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(checker.main(), 1)
        self.assertFalse(os.path.exists(out_path))

    def test_count_flag_prints_counts_and_returns_zero(self):
        live = self.write(f"{ROOT_DIR}/live.zig", 'test "x" {}\n')
        self._stub({ROOT_DIR: {"live.test.x"}}, [live])
        sys.argv = ["prog", "--count"]
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(checker.main(), 0)
        self.assertIn("reachable_test_cases=1", out.getvalue())


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

    def test_depth_gate_consumes_the_reachability_counts(self):
        """One listing, not two: the depth gate reads what --check already produced."""
        self.assertIn("--check --counts-out $(REACHABLE_COUNTS)", self.quality_mk)
        self.assertIn("_lint_zig_test_depth: _lint_zig_test_reachability", self.quality_mk)
        self.assertIn("counts=$$(cat $(REACHABLE_COUNTS))", self.quality_mk)
        self.assertNotIn(
            """find src -name '*.zig' -exec grep -hE '^test "'""",
            self.quality_mk,
            msg="depth gate must not fall back to the textual scan",
        )

    def test_depth_gate_fails_closed_on_a_bad_count(self):
        """The recipe must `set -eu` and reject a non-numeric total: the old form
        printed success with a blank count when the checker errored."""
        recipe = self.quality_mk.split("_lint_zig_test_depth:")[1].split("\n\n")[0]
        self.assertIn("set -eu", recipe)
        self.assertIn("*[!0-9]*", recipe)


if __name__ == "__main__":
    unittest.main(verbosity=2)
