#!/usr/bin/env python3
"""Self-tests for check_zig_test_reachability.py — listing parse and CLI dispatch.

Covers the `zig build list-tests` wire format (each TEST line carries its own root,
so concurrently-run lanes cannot misfile a test), candidate selection, and the
--check/--count/--counts-out surface. Reachability logic lives in the sibling
check_zig_test_reachability_test.py.
"""

import contextlib
import io
import os
import sys
import unittest

from reachability_test_support import (  # noqa: E402
    FakeProc,
    ROOT_DIR,
    ReachabilityTestCase,
    SubprocessFakeCase,
    checker,
)


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

class TestCandidateFiles(ReachabilityTestCase):
    def test_selects_only_zig_files_carrying_a_test_block(self):
        self.write(f"{ROOT_DIR}/with_test.zig", 'test "a" {}\n')
        self.write(f"{ROOT_DIR}/no_test.zig", "pub fn f() void {}\n")
        self.write(f"{ROOT_DIR}/notes.md", 'test "not zig" {}\n')
        self.write("src/lib/deep/nested.zig", 'test "b" {}\n')
        self.assertEqual(
            checker.candidate_files(),
            [f"{ROOT_DIR}/with_test.zig", "src/lib/deep/nested.zig"],
        )

    def test_anonymous_only_file_is_still_a_candidate(self):
        """A file holding only `test { ... }` blocks can be unreachable too. Skipping
        it would leave the gate with the blind spot it exists to close."""
        anon = self.write(f"{ROOT_DIR}/barrel.zig", 'test {\n    _ = @import("x.zig");\n}\n')
        self.assertIn(anon, checker.candidate_files())
        self.assertTrue(checker.declares_a_test(anon))

        plain = self.write(f"{ROOT_DIR}/plain.zig", "pub fn f() void {}\n")
        self.assertFalse(checker.declares_a_test(plain))

    def test_anonymous_only_dead_file_fails_the_gate_with_a_real_count(self):
        anon = self.write(f"{ROOT_DIR}/barrel.zig", 'test {\n    _ = @import("x.zig");\n}\n')
        code, output = self.run_check({ROOT_DIR: set()}, [anon])
        self.assertEqual(code, 1)
        self.assertIn("1 dead block(s)", output)  # not "0", despite zero NAMED blocks

    def test_anonymous_blocks_do_not_inflate_the_depth_count(self):
        """The depth total stays on named blocks so it remains comparable to the
        historical `^test \"` count that Test Baseline was recorded from."""
        path = self.write(f"{ROOT_DIR}/mixed.zig", 'test {\n}\ntest "named" {}\n')
        self.assertEqual(checker.count_blocks(path), (1, 0))

    def test_candidate_files_never_shells_out(self):
        """CI runs in a container where `git ls-files` exits 128 on the checkout.
        Walking the tree keeps the gate working anywhere `python3` runs."""
        self.write(f"{ROOT_DIR}/with_test.zig", 'test "a" {}\n')

        def explode(*args, **kwargs):
            raise AssertionError(f"candidate_files must not spawn {args!r}")

        original = checker.subprocess.run
        checker.subprocess.run = explode
        try:
            self.assertEqual(checker.candidate_files(), [f"{ROOT_DIR}/with_test.zig"])
        finally:
            checker.subprocess.run = original

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

if __name__ == "__main__":
    unittest.main(verbosity=2)
