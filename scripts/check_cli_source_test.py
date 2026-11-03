#!/usr/bin/env python3
"""The CLI's environment-name rule refuses the second spelling it was written for.

    python3 scripts/check_cli_source_test.py

The single-file rules that moved to ast-grep are proved by their own cases
under .ast-grep/rule-tests/.
"""

from __future__ import annotations

import contextlib
import io
import sys
import unittest

sys.dont_write_bytecode = True

import check_cli_source as checker

ENV_TS = 'export const API_URL_ENV = "AGENTSFLEET_API_URL" as const;\n'
GLYPH_TS = '// Reads AGENTSFLEET_API_URL in prose only.\nexport const live = () => palette.pulse("●");\n'
FORMAT_TS = 'import { API_URL_ENV } from "../constants/env.ts";\nexport const heading = (t: string) => palette.pulse(t) + API_URL_ENV;\n'


def clean_tree() -> dict[str, str]:
    return {
        checker.ENV_MODULE: ENV_TS,
        "cli/src/output/glyph.ts": GLYPH_TS,
        "cli/src/output/format.ts": FORMAT_TS,
    }


def lint_tree(files: dict[str, str]) -> tuple[int, str]:
    with checker.lib.fixture_repository(files) as root:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = checker.lib.run([checker.lib.ROOT_FLAG, str(root)], "cli", checker.lint)
        return code, out.getvalue()


def with_file(path: str, text: str) -> dict[str, str]:
    files = clean_tree()
    files[path] = text
    return files


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_a_quoted_read_of_a_declared_name_is_a_second_spelling(self) -> None:
        code, output = lint_tree(with_file("cli/src/lib/api.ts", 'const url = process.env["AGENTSFLEET_API_URL"];\n'))
        self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
        self.assertIn("cli/src/lib/api.ts:1: cli-env-spelling: ", output)

    def test_a_module_that_declares_no_name_is_a_finding(self) -> None:
        code, output = lint_tree({**clean_tree(), checker.ENV_MODULE: "export {};\n"})
        self.assertIn("no environment-variable name was declared", output)

    def test_a_dotted_read_of_a_declared_name_is_a_second_spelling(self) -> None:
        code, output = lint_tree(with_file("cli/src/lib/api.ts", "const url = env.AGENTSFLEET_API_URL;\n"))
        self.assertIn("cli/src/lib/api.ts:1: cli-env-spelling: ", output)

    def test_a_quoted_spelling_of_any_declared_name_is_a_second_spelling(self) -> None:
        declared = ENV_TS + 'export const API_KEY_ENV = "AGENTSFLEET_API_KEY" as const;\n'
        for name in ("AGENTSFLEET_API_URL", "AGENTSFLEET_API_KEY"):
            with self.subTest(name=name):
                files = {**with_file("cli/src/lib/api.ts", f'const value = process.env["{name}"];\n'), checker.ENV_MODULE: declared}
                code, output = lint_tree(files)
                self.assertIn("cli/src/lib/api.ts:1: cli-env-spelling: ", output)

    def test_a_name_inside_a_comment_or_a_longer_name_is_not(self) -> None:
        text = "/* set AGENTSFLEET_API_URL */\nconst other = env.AGENTSFLEET_API_URL_OVERRIDE;\n"
        code, output = lint_tree(with_file("cli/src/lib/api.ts", text))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)


if __name__ == "__main__":
    unittest.main()
