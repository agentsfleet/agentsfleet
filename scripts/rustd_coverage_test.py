#!/usr/bin/env python3
"""Self-tests for rustd_coverage.py.

The merge is only worth having if the union it grades is the union the shards
measured: a line covered by one shard counts, a shard that never reported fails
the lane instead of shrinking it, and the floor is decided exactly at its
boundary. Each test builds lcov text by hand, so no coverage run is needed.

Run: python3 -m unittest discover -s scripts -t scripts -p 'rustd_coverage_test.py'
"""
import contextlib
import io
import os
import subprocess
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

import rustd_coverage as coverage

CORE = "/w/rustd/crates/afd_core/src/lib.rs"
FLEET = "/w/rustd/crates/afd_fleet/src/lease.rs"
REVISION = "0123abcd"


def lcov(path, lines, extra=()):
    body = [f"SF:{path}", *extra, *(f"DA:{line},{hits}" for line, hits in lines), "end_of_record"]
    return "\n".join(["TN:", *body]) + "\n"


def merged_from(*texts):
    merged = {}
    for index, text in enumerate(texts):
        coverage.parse(text, merged, f"shard{index}")
    return merged


class Union(unittest.TestCase):
    def test_a_line_one_shard_covered_is_covered(self):
        merged = merged_from(lcov(CORE, [(1, 0), (2, 0)]), lcov(CORE, [(1, 3), (2, 0)]))
        self.assertEqual(coverage.totals(merged), (1, 2))

    def test_hits_sum_across_shards(self):
        merged = merged_from(lcov(CORE, [(1, 2)]), lcov(CORE, [(1, 5)]))
        self.assertEqual(merged[CORE].lines, {1: 7})

    def test_a_line_only_one_shard_instruments_is_kept(self):
        # A generic instantiated only in one shard's binaries appears in that
        # shard's report alone; dropping it would shrink the denominator.
        merged = merged_from(lcov(CORE, [(1, 1)]), lcov(CORE, [(1, 0), (9, 0)]))
        self.assertEqual(coverage.totals(merged), (1, 2))

    def test_the_same_inputs_render_the_same_bytes_in_any_order(self):
        a = lcov(CORE, [(2, 1), (1, 0)], ["FN:1,f", "FNDA:1,f"])
        b = lcov(FLEET, [(5, 0)])
        self.assertEqual(coverage.render(merged_from(a, b)), coverage.render(merged_from(b, a)))

    def test_inputs_own_totals_are_recomputed_not_trusted(self):
        text = lcov(CORE, [(1, 1), (2, 0)], ["LF:99", "LH:99"])
        rendered = coverage.render(merged_from(text))
        self.assertIn("LF:2\nLH:1\n", rendered)
        self.assertNotIn("LF:99", rendered)

    def test_function_hits_sum_and_keep_their_line(self):
        merged = merged_from(lcov(CORE, [], ["FN:4,f", "FNDA:0,f"]), lcov(CORE, [], ["FN:4,f", "FNDA:2,f"]))
        self.assertEqual((merged[CORE].functions, merged[CORE].function_hits), ({"f": 4}, {"f": 2}))

    def test_a_branch_stays_not_taken_only_while_every_shard_agrees(self):
        merged = merged_from(lcov(CORE, [], ["BRDA:3,0,0,-", "BRDA:3,0,1,-"]), lcov(CORE, [], ["BRDA:3,0,0,4"]))
        self.assertEqual(merged[CORE].branches, {(3, 0, 0): 4, (3, 0, 1): None})


class Refusals(unittest.TestCase):
    def test_an_unknown_record_is_refused_rather_than_dropped(self):
        with self.assertRaisesRegex(coverage.UnusableReport, "VER is not a record"):
            merged_from(lcov(CORE, [(1, 1)], ["VER:2"]))

    def test_a_count_that_is_not_a_number_names_where(self):
        with self.assertRaisesRegex(coverage.UnusableReport, "shard0:3"):
            merged_from("TN:\nSF:x\nDA:1,lots\nend_of_record\n")

    def test_a_record_outside_a_source_file_is_refused(self):
        with self.assertRaisesRegex(coverage.UnusableReport, "outside a source file"):
            merged_from("DA:1,1\n")


class Floor(unittest.TestCase):
    def test_the_boundary_is_exact(self):
        # 975/1000 is exactly 97.5%; float division would put it either side.
        self.assertTrue(coverage.holds(975, 1000, Decimal("97.5")))
        self.assertFalse(coverage.holds(974, 1000, Decimal("97.5")))

    def test_missed_lines_roll_up_by_crate_most_first(self):
        merged = merged_from(lcov(CORE, [(1, 0)]), lcov(FLEET, [(1, 0), (2, 0)]), lcov("/w/build.rs", [(1, 0)]))
        self.assertEqual(
            coverage.missed_by_crate(merged), [("afd_fleet", 2), (coverage.WORKSPACE_ROOT, 1), ("afd_core", 1)]
        )


DIFF = """\
diff --git a/rustd/crates/afd_core/src/lib.rs b/rustd/crates/afd_core/src/lib.rs
--- a/rustd/crates/afd_core/src/lib.rs
+++ b/rustd/crates/afd_core/src/lib.rs
@@ -3,0 +4,2 @@ fn a() {
+    one();
+    two();
@@ -10 +12 @@ fn b() {
-    old();
+    new();
diff --git a/rustd/crates/afd_core/src/gone.rs b/rustd/crates/afd_core/src/gone.rs
--- a/rustd/crates/afd_core/src/gone.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn gone() {}
-
"""
CORE_RELATIVE = "rustd/crates/afd_core/src/lib.rs"


class Patch(unittest.TestCase):
    def test_added_lines_are_read_per_file_and_deletions_add_nothing(self):
        self.assertEqual(coverage.changed_lines(DIFF), {CORE_RELATIVE: {4, 5, 12}})

    def test_only_instrumented_changed_lines_are_graded(self):
        # Line 5 is a brace llvm-cov does not instrument: coverable on neither side.
        merged = merged_from(lcov("/w/" + CORE_RELATIVE, [(4, 1), (12, 0), (30, 0)]))
        self.assertEqual(coverage.patch_grade(merged, {CORE_RELATIVE: {4, 5, 12}}), (1, 2, [f"{CORE_RELATIVE}:12"]))

    def test_a_changed_file_the_report_does_not_hold_is_not_coverable(self):
        merged = merged_from(lcov(CORE, [(1, 0)]))
        self.assertEqual(coverage.patch_grade(merged, {"rustd/crates/afd_bench/src/main.rs": {1}}), (0, 0, []))

    def test_ninety_nine_of_a_hundred_holds_the_patch_floor(self):
        self.assertTrue(coverage.holds(99, 100, Decimal("99")))
        self.assertFalse(coverage.holds(98, 100, Decimal("99")))


class Main(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.out = self.dir / "lcov.info"

    def shard(self, name, text, revision=REVISION):
        path = self.dir / f"lcov-{name}.info"
        path.write_text(text, encoding="utf-8")
        if revision is not None:
            path.with_suffix(coverage.REVISION_SUFFIX).write_text(revision + "\n", encoding="utf-8")
        return str(path)

    def run_main(self, *argv):
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            status = coverage.main(list(argv))
        return status, stdout.getvalue() + stderr.getvalue()

    def test_the_union_passes_where_no_shard_alone_would(self):
        a = self.shard("a", lcov(CORE, [(1, 1), (2, 0)]))
        b = self.shard("b", lcov(CORE, [(1, 0), (2, 1)]))
        status, said = self.run_main("--floor", "97.5", "--revision", REVISION, "--out", str(self.out), a, b)
        self.assertEqual(status, 0, said)
        self.assertIn("100.0000% >= 97.5% floor — 2 of 2 lines covered", said)
        self.assertIn("LH:2", self.out.read_text(encoding="utf-8"))

    def test_a_missed_floor_exits_one_and_names_the_crates(self):
        a = self.shard("a", lcov(FLEET, [(1, 1), (2, 0)]))
        status, said = self.run_main("--floor", "97.5", "--out", str(self.out), a)
        self.assertEqual(status, coverage.EXIT_FLOOR_MISSED)
        self.assertIn("50.0000% < 97.5% floor", said)
        self.assertIn("1  afd_fleet", said)

    def test_a_shard_that_never_reported_fails_the_lane(self):
        a = self.shard("a", lcov(CORE, [(1, 1)]))
        missing = str(self.dir / "lcov-b.info")
        status, said = self.run_main("--floor", "50", "--out", str(self.out), a, missing)
        self.assertEqual(status, coverage.EXIT_UNUSABLE)
        self.assertIn("lcov-b.info is missing", said)
        self.assertFalse(self.out.exists())

    def test_a_shard_from_another_commit_is_refused(self):
        a = self.shard("a", lcov(CORE, [(1, 1)]), revision="feedface")
        status, said = self.run_main("--floor", "50", "--revision", REVISION, "--out", str(self.out), a)
        self.assertEqual(status, coverage.EXIT_UNUSABLE)
        self.assertIn("measured at feedface", said)

    def test_a_shard_without_a_sidecar_is_refused_when_a_revision_is_asked(self):
        a = self.shard("a", lcov(CORE, [(1, 1)]), revision=None)
        status, said = self.run_main("--floor", "50", "--revision", REVISION, "--out", str(self.out), a)
        self.assertEqual(status, coverage.EXIT_UNUSABLE)
        self.assertIn("(no sidecar)", said)

    def test_reports_with_no_lines_are_not_a_pass(self):
        a = self.shard("a", "TN:\n")
        status, said = self.run_main("--floor", "0", "--out", str(self.out), a)
        self.assertEqual(status, coverage.EXIT_UNUSABLE)
        self.assertIn("hold no lines", said)

    def test_a_floor_that_is_not_a_number_is_refused(self):
        a = self.shard("a", lcov(CORE, [(1, 1)]))
        status, said = self.run_main("--floor", "ninety", "--out", str(self.out), a)
        self.assertEqual(status, coverage.EXIT_UNUSABLE)
        self.assertIn("not a number", said)


class PatchMain(unittest.TestCase):
    """The patch grade end to end, against a real repository's diff."""

    def setUp(self):
        self.repo = Path(tempfile.mkdtemp())
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(self.repo)
        source = self.repo / "rustd/crates/afd_core/src/lib.rs"
        source.parent.mkdir(parents=True)
        source.write_text("fn a() {}\n", encoding="utf-8")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("commit", "-qm", "base")
        self.base = self.git("rev-parse", "HEAD").strip()
        source.write_text("fn a() {}\nfn b() {}\nfn c() {}\n", encoding="utf-8")
        self.git("commit", "-qam", "change")

    def git(self, *args):
        identity = ["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"]
        return subprocess.run(["git", *identity, *args], check=True, capture_output=True, text=True).stdout

    def grade(self, lines, *extra):
        report = self.repo / "lcov-a.info"
        report.write_text(lcov(str(self.repo / CORE_RELATIVE), lines), encoding="utf-8")
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stdout):
            status = coverage.main(["--floor", "0", "--out", str(self.repo / "lcov.info"), *extra, str(report)])
        return status, stdout.getvalue()

    def test_an_unhit_added_line_misses_the_patch_floor_and_is_named(self):
        status, said = self.grade([(1, 1), (2, 1), (3, 0)], "--patch-base", self.base)
        self.assertEqual(status, coverage.EXIT_FLOOR_MISSED)
        self.assertIn("50.0000% < 99% floor — 1 of 2 changed lines covered", said)
        self.assertIn(f"{CORE_RELATIVE}:3", said)

    def test_every_added_line_hit_holds_it(self):
        status, said = self.grade([(1, 0), (2, 1), (3, 1)], "--patch-base", self.base)
        self.assertEqual(status, 0, said)
        self.assertIn("patch coverage 100.0000% >= 99% floor", said)

    def test_a_base_git_cannot_resolve_is_refused(self):
        status, said = self.grade([(1, 1)], "--patch-base", "no-such-revision")
        self.assertEqual(status, coverage.EXIT_UNUSABLE)
        self.assertIn("git diff no-such-revision HEAD failed", said)


if __name__ == "__main__":
    unittest.main()
