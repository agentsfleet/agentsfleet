#!/usr/bin/env python3
"""The Rust coverage judge grades the patch and every file it touches.

    python3 scripts/rustd_coverage_test.py
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import unittest.mock
from decimal import Decimal
from pathlib import Path

sys.dont_write_bytecode = True

import rustd_coverage as judge

SCRIPT = Path(__file__).resolve().with_name("rustd_coverage.py")
GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]


def isolated_env():
    """This process's environment without git's repository pointers.

    A git hook exports GIT_DIR, GIT_INDEX_FILE and friends. Inherited, they
    point every git call here at the hook's repository instead of the
    scratch one, and the scratch commits overwrite that repository's index.
    """
    return {name: value for name, value in os.environ.items() if not name.startswith("GIT_")}


def lcov(records):
    """lcov text from {path: {line: hits}}."""
    out = ["TN:"]
    for path, lines in records.items():
        out.append(f"SF:{path}")
        out.extend(f"DA:{line},{hits}" for line, hits in lines.items())
        out.append("end_of_record")
    return "\n".join(out) + "\n"


def merged(records):
    result = {}
    judge.parse(lcov(records), result, "fixture")
    return result


class FileGrades(unittest.TestCase):
    def test_a_touched_file_is_graded_on_every_line_not_only_the_changed_ones(self):
        report = merged({"/w/crates/a/src/lib.rs": {1: 1, 2: 0, 3: 1, 4: 1}})
        grades = judge.file_grades(report, {"crates/a/src/lib.rs": {1}})
        self.assertEqual(grades, [("crates/a/src/lib.rs", 3, 4, [2])])

    def test_a_file_the_report_does_not_measure_is_not_graded(self):
        report = merged({"/w/crates/a/src/lib.rs": {1: 1}})
        self.assertEqual(judge.file_grades(report, {"crates/a/tests/suite.rs": {1}}), [])

    def test_a_measured_file_with_no_instrumented_lines_is_not_graded(self):
        report = {"/w/crates/a/src/empty.rs": judge.SourceFile()}
        self.assertEqual(judge.file_grades(report, {"crates/a/src/empty.rs": {1}}), [])

    def test_an_untouched_file_is_not_graded_however_low(self):
        report = merged({"/w/crates/a/src/old.rs": {1: 0}, "/w/crates/a/src/new.rs": {1: 1}})
        grades = judge.file_grades(report, {"crates/a/src/new.rs": {1}})
        self.assertEqual([grade[0] for grade in grades], ["crates/a/src/new.rs"])


class ReportFiles(unittest.TestCase):
    def run_report(self, grades, floor="99"):
        with unittest.mock.patch("builtins.print") as printed:
            verdict = judge.report_files(grades, Decimal(floor))
        return verdict, "\n".join(" ".join(map(str, call.args)) for call in printed.call_args_list)

    def test_every_file_at_the_floor_passes(self):
        verdict, output = self.run_report([("a.rs", 99, 100, [7]), ("b.rs", 5, 5, [])])
        self.assertEqual(verdict, 0)
        self.assertIn("all 2 measured files", output)

    def test_one_file_under_the_floor_fails_and_names_its_unhit_lines(self):
        verdict, output = self.run_report([("a.rs", 98, 100, [3, 9]), ("b.rs", 5, 5, [])])
        self.assertEqual(verdict, judge.EXIT_FLOOR_MISSED)
        self.assertIn("1 of 2 files", output)
        self.assertIn("a.rs  unhit: 3,9", output)
        self.assertNotIn("b.rs", output)

    def test_long_lists_are_cut_and_say_how_much_is_hidden(self):
        unhit = list(range(1, judge.UNHIT_SHOWN + 6))
        failing = [(f"f{index}.rs", 0, len(unhit), unhit) for index in range(judge.UNHIT_SHOWN + 2)]
        verdict, output = self.run_report(failing)
        self.assertEqual(verdict, judge.EXIT_FLOOR_MISSED)
        self.assertIn("… +5", output)
        self.assertIn("… and 2 more files", output)


class CommandLine(unittest.TestCase):
    """The judge as the make target runs it, against a real repository."""

    def setUp(self):
        workspace = tempfile.TemporaryDirectory()
        self.addCleanup(workspace.cleanup)
        self.root = Path(workspace.name)
        self.git("init", "-q")
        source = self.root / "crates/a/src"
        source.mkdir(parents=True)
        (source / "kept.rs").write_text("fn a() {}\nfn b() {}\n")
        (source / "moved.rs").write_text("fn m() {}\n")
        self.git("add", ".")
        self.git("commit", "-qm", "base")
        self.base = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.run(GIT + list(args), cwd=self.root, check=True, capture_output=True, text=True,
                              env=isolated_env()).stdout

    def change(self):
        (self.root / "crates/a/src/kept.rs").write_text("fn a() {}\nfn b() {}\nfn c() {}\n")
        (self.root / "crates/a/src/moved").mkdir()
        self.git("mv", "crates/a/src/moved.rs", "crates/a/src/moved/mod.rs")
        self.git("commit", "-qam", "change")

    def judge(self, records, *extra):
        report = self.root / "lcov-all.info"
        report.write_text(lcov(records))
        command = [sys.executable, str(SCRIPT), "--floor", "0", "--out", str(self.root / "lcov.info"), *extra, str(report)]
        return subprocess.run(command, cwd=self.root, capture_output=True, text=True, env=isolated_env())

    def test_a_hooks_git_environment_never_reaches_the_scratch_repository(self):
        foreign = self.root / "foreign"
        foreign.mkdir()
        hook_env = {"GIT_DIR": str(foreign / ".git"), "GIT_INDEX_FILE": str(foreign / "index")}
        with mock.patch.dict(os.environ, hook_env):
            self.change()
        self.assertFalse((foreign / "index").exists())
        self.assertEqual(self.git("rev-list", "--count", "HEAD").strip(), "2")

    def test_a_touched_file_under_the_floor_fails_the_lane(self):
        self.change()
        kept = str(self.root / "crates/a/src/kept.rs")
        result = self.judge({kept: {1: 0, 2: 1, 3: 1}}, "--patch-base", self.base, "--per-file-floor", "99")
        self.assertEqual(result.returncode, judge.EXIT_FLOOR_MISSED, result.stdout)
        self.assertIn("crates/a/src/kept.rs  unhit: 1", result.stdout)

    def test_a_pure_move_is_a_touched_file_and_is_graded(self):
        self.change()
        records = {str(self.root / "crates/a/src/kept.rs"): {1: 1, 2: 1, 3: 1},
                   str(self.root / "crates/a/src/moved/mod.rs"): {1: 0}}
        result = self.judge(records, "--patch-base", self.base, "--per-file-floor", "99")
        self.assertEqual(result.returncode, judge.EXIT_FLOOR_MISSED, result.stdout)
        self.assertIn("1 of 2 files", result.stdout)
        self.assertIn("crates/a/src/moved/mod.rs  unhit: 1", result.stdout)

    def test_a_covered_pure_move_passes(self):
        self.change()
        records = {str(self.root / "crates/a/src/kept.rs"): {1: 1, 2: 1, 3: 1},
                   str(self.root / "crates/a/src/moved/mod.rs"): {1: 1}}
        result = self.judge(records, "--patch-base", self.base, "--per-file-floor", "99")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("all 2 measured files", result.stdout)

    def test_the_per_file_floor_needs_a_patch_base(self):
        result = self.judge({str(self.root / "crates/a/src/kept.rs"): {1: 1}}, "--per-file-floor", "99")
        self.assertEqual(result.returncode, judge.EXIT_UNUSABLE)
        self.assertIn("needs --patch-base", result.stderr)

    def test_a_per_file_floor_that_is_not_a_number_is_refused(self):
        result = self.judge({str(self.root / "crates/a/src/kept.rs"): {1: 1}},
                            "--patch-base", self.base, "--per-file-floor", "ninety")
        self.assertEqual(result.returncode, judge.EXIT_UNUSABLE)
        self.assertIn("--per-file-floor 'ninety'", result.stderr)

    def test_without_the_option_no_file_is_graded_whole(self):
        self.change()
        kept = str(self.root / "crates/a/src/kept.rs")
        result = self.judge({kept: {1: 0, 2: 1, 3: 1}}, "--patch-base", self.base, "--patch-floor", "0")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertNotIn("per-file", result.stdout)


if __name__ == "__main__":
    unittest.main()
