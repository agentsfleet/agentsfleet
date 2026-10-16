#!/usr/bin/env python3
"""Each dashboard source rule refuses the regression it was written for.

    python3 scripts/check_app_source_test.py

Covers check_app_source.py and the identity-provider import boundary in
ui/packages/app/.oxlintrc.json.
"""

from __future__ import annotations

import contextlib
import io
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

import check_app_source as checker

APP = checker.APP


def listed(stdout: str, returncode: int = 0, stderr: str = "") -> subprocess.CompletedProcess[str]:
    return subprocess.CompletedProcess(args=[], returncode=returncode, stdout=stdout, stderr=stderr)


LISTING = listed(f"  [journeys] › {checker.EXECUTION_SPEC}:12:3 › a fleet executes\nTotal: 1 test in 1 file\n")

EVENTS_TS = 'export const EVENTS = {\n  fleet_created: "fleet_created",\n} as const;\n'
SPEC_TS = 'await page.waitForLoadState("load");\n'
CLIENT_TSX = '"use client";\nexport function Panel({ label }: { label: string }) {\n  const tokenCount = 3;\n  return <p>{label}{tokenCount}</p>;\n}\n'
SERVER_TS = 'const bearer = await getToken();\nexport const headers = { Authorization: `Bearer ${bearer}` };\n'
HANDOFF_TSX = '"use client";\nconst token = await getToken({ template: "api" });\n'
CHARGES_TS = 'export const label = (at: Date) => at.toLocaleDateString("en-US");\n'
ZONE_TS = "export const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;\n"


def clean_tree() -> dict[str, str]:
    return {
        checker.EVENTS_MODULE: EVENTS_TS,
        f"{APP}/components/track.ts": 'import { EVENTS } from "@/lib/analytics/events";\ncapture(EVENTS.fleet_created);\n',
        f"{checker.ACCEPTANCE}/install.spec.ts": SPEC_TS,
        f"{APP}/components/Panel.tsx": CLIENT_TSX,
        f"{APP}/lib/actions/with-token.ts": SERVER_TS,
        checker.CLI_HANDOFF: HANDOFF_TSX,
        checker.DATE_LABEL_HOME: CHARGES_TS,
        f"{APP}/lib/zone.ts": ZONE_TS,
    }


def lint_tree(files: dict[str, str], listing: subprocess.CompletedProcess[str] | None = LISTING) -> tuple[int, str]:
    original = checker.list_journeys
    checker.list_journeys = lambda root: listing
    try:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative_path, text in files.items():
                (root / relative_path).parent.mkdir(parents=True, exist_ok=True)
                (root / relative_path).write_text(text, encoding="utf-8")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = checker.lib.run([checker.lib.ROOT_FLAG, str(root)], "app", checker.lint)
            return code, out.getvalue()
    finally:
        checker.list_journeys = original


def with_file(path: str, text: str) -> dict[str, str]:
    files = clean_tree()
    files[path] = text
    return files


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_moved_rules_reject_seeded_violations(self) -> None:
        spec = f"{checker.ACCEPTANCE}/install.spec.ts"
        cases = {
            "app-acceptance-networkidle": with_file(spec, SPEC_TS.replace('"load"', '"networkidle"')),
            "app-event-literal": with_file(f"{APP}/tests/track.test.ts", 'expect(name).toBe("fleet_created");\n'),
            "app-api-template": with_file(f"{APP}/app/login/page.tsx", 'const t = await getToken({ template: "api" });\n'),
            "app-client-credential": with_file(f"{APP}/components/Panel.tsx", CLIENT_TSX.replace("{ label }: { label: string }", "{ token }: { token: string }")),
            "app-date-formatter": with_file(f"{APP}/lib/when.ts", "export const when = (d: Date) => d.toLocaleString();\n"),
        }
        for rule, files in cases.items():
            with self.subTest(rule=rule):
                code, output = lint_tree(files)
                self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f": {rule}: ", output)

    def test_a_journeys_project_that_drops_the_execution_spec_is_refused(self) -> None:
        code, output = lint_tree(clean_tree(), listing=listed("  [journeys] › other.spec.ts:1:1 › x\n"))
        self.assertIn(": app-journey-listed: the journeys project does not run", output)
        code, output = lint_tree(clean_tree(), listing=None)
        self.assertIn("Playwright is not installed", output)

    def test_a_listing_that_fails_reports_playwrights_error_not_a_missing_journey(self) -> None:
        error = "Error: NEXT_PUBLIC_API_URL is unset — refusing to guess a backend."
        code, output = lint_tree(clean_tree(), listing=listed("Total: 0 tests in 0 files\n", returncode=1, stderr=error))
        self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
        self.assertIn(f": app-journey-listed: Playwright could not list the journeys project: {error}", output)
        self.assertNotIn("does not run", output)

    def test_a_retired_formatter_named_only_in_a_comment_is_not_a_finding(self) -> None:
        code, output = lint_tree(with_file(f"{APP}/lib/when.ts", "// formatDate was deleted.\nexport const x = 1;\n"))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)
        code, output = lint_tree(with_file(f"{APP}/lib/when.ts", "export const formatDate = 1;\n"))
        self.assertIn(f"{APP}/lib/when.ts:1: app-date-formatter: ", output)


class CommentBlankingKeepsLiteralsWhole(unittest.TestCase):
    """A quote inside JSX text or a pattern must not hide the comments after it."""

    def test_a_comment_after_a_jsx_apostrophe_is_still_a_comment(self) -> None:
        source = "export const P = () => <p>Don't panic</p>;\n// formatDate was deleted.\nexport const x = 1;\n"
        code, output = lint_tree(with_file(f"{APP}/lib/when.tsx", source))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_a_comment_after_a_pattern_holding_a_quote_is_still_a_comment(self) -> None:
        source = "export const missing = /Couldn't find \"it\"/i;\n// formatDate was deleted.\nexport const x = 1;\n"
        code, output = lint_tree(with_file(f"{APP}/lib/when.ts", source))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_code_after_a_jsx_apostrophe_is_still_checked(self) -> None:
        source = "export const P = () => <p>Don't panic</p>;\n// note\nexport const formatDate = 1;\n"
        code, output = lint_tree(with_file(f"{APP}/lib/when.tsx", source))
        self.assertIn(f"{APP}/lib/when.tsx:3: app-date-formatter: ", output)

    def test_slashes_inside_a_pattern_are_kept_and_a_division_still_ends_before_its_comment(self) -> None:
        pattern = "const slashes = path.replace(/\\//g, \"-\");\n"
        self.assertEqual(pattern, checker.lib.blank_js_comments(pattern))
        division = "const half = total / 2; // formatDate\n"
        self.assertEqual("const half = total / 2; " + " " * len("// formatDate") + "\n", checker.lib.blank_js_comments(division))


OXLINT = checker.lib.ROOT / APP / "node_modules/.bin/oxlint"
OXLINT_CONFIG = checker.lib.ROOT / APP / ".oxlintrc.json"
# Pinned: oxlint picks its report format from the environment (GitHub annotations
# on a runner, an agent format under a coding agent), so the assertions read one.
OXLINT_FORMAT = "--format=unix"
RESTRICTED_IMPORTS = re.escape("eslint(no-restricted-imports)")


class IdentityProviderBoundary(unittest.TestCase):
    def lint(self, files: dict[str, str]) -> str:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copy(OXLINT_CONFIG, root / ".oxlintrc.json")
            for relative_path, text in files.items():
                (root / relative_path).parent.mkdir(parents=True, exist_ok=True)
                (root / relative_path).write_text(text, encoding="utf-8")
            result = subprocess.run([str(OXLINT), "-c", ".oxlintrc.json", OXLINT_FORMAT, *files], cwd=root, capture_output=True, text=True, check=False)
            return result.stdout + result.stderr

    def test_moved_rules_reject_seeded_violations(self) -> None:
        sdk_auth = 'import { auth } from "@clerk/nextjs/server";\nexport const a = auth;\n'
        middleware = 'import { clerkMiddleware } from "@clerk/nextjs/server";\nexport const m = clerkMiddleware;\n'
        output = self.lint({
            "app/page.ts": sdk_auth,
            "proxy.ts": sdk_auth,
            "lib/auth/credential.ts": sdk_auth,
            "lib/middleware-copy.ts": middleware,
        })
        self.assertRegex(output, rf"(?m)^app/page\.ts:1:1: .*\[Error/{RESTRICTED_IMPORTS}\]$")
        self.assertRegex(output, rf"(?m)^lib/middleware-copy\.ts:1:1: .*\[Error/{RESTRICTED_IMPORTS}\]$")
        self.assertRegex(output, rf"(?m)^proxy\.ts:1:10: 'auth' import .*\[Error/{RESTRICTED_IMPORTS}\]$")
        self.assertNotIn("lib/auth/credential.ts:", output)


if __name__ == "__main__":
    unittest.main()
