#!/usr/bin/env python3
"""Each website copy rule refuses the line it was written for.

    python3 scripts/check_website_copy_test.py
"""

from __future__ import annotations

import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

import check_website_copy as web

HOME = f"{web.WEBSITE_SRC}/pages/Home.tsx"
HOME_TSX = "export const Home = () => <h1>AI agents that wake when production breaks.</h1>;\n"


def lint_home(text: str) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / HOME).parent.mkdir(parents=True)
        (root / HOME).write_text(text, encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = web.lib.run([web.lib.ROOT_FLAG, str(root)], "website", web.lint)
        return code, out.getvalue()


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_home(HOME_TSX)
        self.assertEqual(web.lib.EXIT_CLEAN, code, output)

    def test_moved_rules_reject_seeded_violations(self) -> None:
        cases = {
            "web-retired-framing": HOME_TSX.replace("AI agents", "Validated PR delivery for AI agents"),
            "web-unvalidated-claim": HOME_TSX.replace("production breaks.", "production breaks, with Zero Tickets."),
            "web-retired-noun": HOME_TSX.replace("AI agents", "AI " + web.RETIRED_NOUN + "s"),
        }
        for rule, text in cases.items():
            with self.subTest(rule=rule):
                code, output = lint_home(text)
                self.assertEqual(web.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f"{HOME}:1: {rule}: ", output)

    def test_a_longer_word_containing_the_retired_noun_is_not_a_finding(self) -> None:
        code, output = lint_home(HOME_TSX.replace("AI agents", web.RETIRED_NOUN + "land agents"))
        self.assertEqual(web.lib.EXIT_CLEAN, code, output)

    def test_an_empty_source_tree_is_a_finding(self) -> None:
        with tempfile.TemporaryDirectory() as directory, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(web.lib.EXIT_FINDINGS, web.lib.run([web.lib.ROOT_FLAG, directory], "website", web.lint))


if __name__ == "__main__":
    unittest.main()
