#!/usr/bin/env python3
"""Self-tests for check_route_registration_doc.py's make-target scans.

Run directly, or via the make target:

    python3 scripts/check_route_registration_doc_test.py
    make check-route-registration-doc

Both make-target regexes were underscore-blind, which made every internal
`_`-prefixed target invisible: a phantom `make _no_such_target` cited in the
guide was reported clean, and `_lint_zig_test_depth:` matched no definition at
all. Worse, the definition regex kept its `_?` outside the capture group, so
`_fmt:` registered under the name `fmt` — inverting the check, since `make fmt`
(nonexistent) passed while `make _fmt` (real) was reported phantom.

These are the tests that catch a revert. A citation planted in the guide cannot:
the narrowed regex simply would not capture it, and an uncaptured citation is
never checked.
"""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import check_route_registration_doc as checker  # noqa: E402

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MAKE_DIR = os.path.join(REPO_ROOT, checker.MAKE_DIR)
MAKEFILE = os.path.join(REPO_ROOT, checker.MAKEFILE_PATH)
REST_GUIDE = os.path.join(REPO_ROOT, checker.REST_GUIDE_PATH)

# Real internal targets: one underscore-prefixed, one with inner underscores.
FMT_CHECK_TARGET = "_fmt_check"
TEST_DEPTH_TARGET = "_lint_zig_test_depth"


class TestUnderscoreTargets(unittest.TestCase):
    def test_underscore_targets_captured(self):
        doc = f"Run `make {FMT_CHECK_TARGET}` before pushing."
        self.assertEqual(checker.DOC_MAKE_TARGET_RE.findall(doc), [FMT_CHECK_TARGET])

        definitions = f"{TEST_DEPTH_TARGET}: check-test-reachability\n"
        self.assertEqual(
            checker.MAKE_TARGET_DEF_RE.findall(definitions), [TEST_DEPTH_TARGET]
        )

    def test_underscore_prefix_stays_in_the_captured_name(self):
        # `^_?([a-z]...)` captured `fmt` from `_fmt:`, so `make fmt` — which does
        # not exist — passed, and `make _fmt` — which does — was called phantom.
        captured = checker.MAKE_TARGET_DEF_RE.findall("_fmt:\n\tzig fmt .\n")
        self.assertEqual(captured, ["_fmt"])
        self.assertNotIn("fmt", captured)

    def test_phantom_underscore_target_flagged(self):
        doc = "Run `make _no_such_target` to do the thing."
        violations = checker.check_phantom_make_targets(doc, MAKE_DIR, MAKEFILE)
        self.assertEqual(violations, ["PHANTOM TARGET: _no_such_target"])

    def test_real_underscore_targets_resolve(self):
        doc = f"Run `make {FMT_CHECK_TARGET}` and `make {TEST_DEPTH_TARGET}`."
        # Assert the citations are seen before asserting they resolve. An
        # underscore-blind regex captures nothing, and a doc that cites nothing
        # trivially has no phantom targets — a pass that proves the opposite.
        self.assertEqual(
            checker.DOC_MAKE_TARGET_RE.findall(doc), [FMT_CHECK_TARGET, TEST_DEPTH_TARGET]
        )
        self.assertEqual(checker.check_phantom_make_targets(doc, MAKE_DIR, MAKEFILE), [])


class TestRealCorpus(unittest.TestCase):
    def test_make_target_set_includes_underscore_names(self):
        targets = checker.real_make_targets(MAKE_DIR, MAKEFILE)
        self.assertIn(FMT_CHECK_TARGET, targets)
        self.assertIn(TEST_DEPTH_TARGET, targets)
        # Guards a vacuous pass: an empty set would satisfy no assertion above.
        self.assertGreater(len(targets), 10)

    def test_rest_guide_cited_targets_all_resolve(self):
        doc_text = checker.read_file(REST_GUIDE)
        self.assertIsNotNone(doc_text, f"{REST_GUIDE} not found")
        cited = set(checker.DOC_MAKE_TARGET_RE.findall(doc_text))
        self.assertTrue(cited, "guide cites no make targets — the scan proves nothing")
        self.assertEqual(checker.check_phantom_make_targets(doc_text, MAKE_DIR, MAKEFILE), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
