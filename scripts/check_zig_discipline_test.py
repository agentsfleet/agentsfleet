#!/usr/bin/env python3
"""Self-tests for `lint-zig.py --discipline` (the ghostty-derived A5/A2 checks).

Drives the real CLI contract as a subprocess against seeded fixtures under
tests/lint/, proving each check bites and that the roster scopes blocking vs
advisory. Run via: python3 -m unittest discover -s scripts -t scripts \\
    -p 'check_zig_discipline*_test.py'
"""
import re
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

# C4 mutex-doc audit (Dimension 4.2): every mutex/wire field in the discipline
# base carries an invariant doc comment. The `\w*(mutex|wire|lock)` field-name
# anchor excludes the Mutex wrapper's own `inner` field in sync.zig.
BASE_PREFIXES = (
    "src/agentsfleetd/cmd", "src/agentsfleetd/events", "src/agentsfleetd/fleet",
    "src/agentsfleetd/queue", "src/runner/daemon", "src/runner/child_supervisor",
    "src/lib",
)
MUTEX_DECL = re.compile(r"^\s*\w*(?:mutex|wire|lock):\s*[\w.]*(?:Mutex|RwLock)\b")


def _base_zig_files():
    files = []
    for pre in BASE_PREFIXES:
        p = ROOT / pre
        if p.is_dir():
            files += [f for f in p.rglob("*.zig")
                      if "_test" not in f.name and f.name != "tests.zig"]
        else:  # file-name-prefix roster entry (child_supervisor)
            files += [f for f in p.parent.glob(p.name + "*.zig")
                      if "_test" not in f.name]
    return files


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

    def test_base_mutexes_documented(self):
        # C4 (Dimension 4.2): every mutex/wire in the discipline base carries an
        # invariant doc comment (on the line above or trailing the declaration).
        undocumented = []
        count = 0
        for f in _base_zig_files():
            lines = f.read_text().splitlines()
            depth = 0
            test_depth = None  # brace depth at which the enclosing `test` block opened
            for i, line in enumerate(lines):
                entering_test = test_depth is None and re.match(r"^\s*test\b.*\{", line)
                if entering_test:
                    test_depth = depth
                depth += line.count("{") - line.count("}")
                if test_depth is not None:
                    if depth <= test_depth:
                        test_depth = None  # left the test block
                    continue  # C4 governs production aggregates, not test helpers
                if MUTEX_DECL.match(line):
                    count += 1
                    above = lines[i - 1].strip() if i > 0 else ""
                    if not (above.startswith("//") or "//" in line):
                        undocumented.append(f"{f.relative_to(ROOT)}:{i + 1}: {line.strip()}")
        self.assertEqual(undocumented, [],
                         f"base mutexes without an invariant doc comment: {undocumented}")
        self.assertGreaterEqual(count, 4, f"expected >=4 base mutexes, found {count}")


if __name__ == "__main__":
    unittest.main()
