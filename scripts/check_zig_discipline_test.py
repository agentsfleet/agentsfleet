#!/usr/bin/env python3
"""Self-tests for `lint-zig.py --discipline` (the ghostty-derived A5/A2 checks).

Drives the real CLI contract as a subprocess against seeded fixtures under
tests/lint/, proving each check bites and that the roster scopes blocking vs
advisory. Run via: python3 -m unittest discover -s scripts -t scripts \\
    -p 'check_zig_discipline*_test.py'
"""
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROSTER = "tests/lint/fixtures_roster.txt"
IN_ROSTER = "tests/lint/in_roster"
OUT_OF_ROSTER = "tests/lint/out_of_roster"
CLEAN = "tests/lint/clean"
ADVISORY = "tests/lint/advisory"


def run(*relpaths, list_warnings=False):
    cmd = [sys.executable, "lint-zig.py", "--discipline", "--roster", ROSTER]
    if list_warnings:
        cmd.append("--list")
    cmd += list(relpaths)
    return subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True)


class DisciplineLint(unittest.TestCase):
    def test_lint_detects_missing_poison(self):
        r = run(IN_ROSTER)
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("A5-POISON", r.stdout)
        self.assertIn("bad_poison.zig", r.stdout)

    def test_lint_detects_missing_ownership_phrase(self):
        r = run(IN_ROSTER)
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("A5-PHRASE", r.stdout)
        self.assertIn("bad_phrase.zig", r.stdout)

    def test_lint_roster_scoping(self):
        # Same freeing-deinit-without-poison violation: BLOCKS inside the roster,
        # only WARNS outside it. The fixture roster carries comments + blank lines,
        # so a green run also proves roster-parse tolerance.
        inside = run(IN_ROSTER, list_warnings=True)
        self.assertEqual(inside.returncode, 1, inside.stdout)
        outside = run(OUT_OF_ROSTER, list_warnings=True)
        self.assertEqual(outside.returncode, 0, outside.stdout)
        self.assertIn("A5-POISON", outside.stdout)
        self.assertIn("warn", outside.stdout)

    def test_lint_clean_tree_passes(self):
        r = run(CLEAN)
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertIn("passed", r.stdout)
        self.assertNotIn("A5-POISON", r.stdout)
        self.assertNotIn("A5-PHRASE", r.stdout)

    def test_lint_warns_multi_try_no_errdefer(self):
        r = run(ADVISORY, list_warnings=True)
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertIn("A2-ERRDEFER", r.stdout)


if __name__ == "__main__":
    unittest.main()
