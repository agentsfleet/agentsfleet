#!/usr/bin/env python3
"""Each CLI source rule refuses the second spelling it was written for.

    python3 scripts/check_cli_source_test.py
"""

from __future__ import annotations

import contextlib
import io
import sys
import unittest

sys.dont_write_bytecode = True

import check_cli_source as checker

ENV_TS = 'export const API_URL_ENV = "AGENTSFLEET_API_URL" as const;\n'
PALETTE_TS = 'export const pulse = (text: string) => `\\u001b[38;5;51m${text}\\u001b[0m`;\n'
GLYPH_TS = '// Reads AGENTSFLEET_API_URL in prose only.\nexport const live = () => palette.pulse("●");\n'
FORMAT_TS = 'import { API_URL_ENV } from "../constants/env.ts";\nexport const heading = (t: string) => palette.pulse(t) + API_URL_ENV;\n'
BANNER_TS = 'export const banner = "agentsfleet";\n'
CONNECTOR_TS = "yield* output.printTable(rows);\n"
LIST_TS = "yield* printEntityTable(spec, rows);\n"
CITING_TEST = "// See rustd/crates/afd_core/src/env.rs for the knob.\n"


def clean_tree() -> dict[str, str]:
    return {
        checker.ENV_MODULE: ENV_TS,
        checker.PALETTE: PALETTE_TS,
        "cli/src/output/glyph.ts": GLYPH_TS,
        "cli/src/output/format.ts": FORMAT_TS,
        checker.BANNER: BANNER_TS,
        f"{checker.COMMANDS}/connector.ts": CONNECTOR_TS,
        f"{checker.COMMANDS}/fleet.ts": LIST_TS,
        "cli/test/env.unit.test.ts": CITING_TEST,
        "rustd/crates/afd_core/src/env.rs": "pub const LOG_LEVEL_VAR: &str = \"AGENTSFLEET_LOG_LEVEL\";\n",
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

    def test_moved_rules_reject_seeded_violations(self) -> None:
        cases = {
            "cli-env-spelling": ("cli/src/lib/api.ts", 'const url = process.env["AGENTSFLEET_API_URL"];\n'),
            "cli-raw-escape": ("cli/src/lib/spinner.ts", 'const red = "\\x1b[31m";\n'),
            "cli-retired-glyph": ("cli/src/lib/done.ts", 'const done = "\U0001F389 done";\n'),
            "cli-pulse-callers": ("cli/src/lib/a.ts", "palette.pulse(a); palette.pulse(b);\n"),
            "cli-detail-table": (f"{checker.COMMANDS}/fleet.ts", "yield* output.printTable(rows);\n"),
            "cli-retired-citation": ("cli/test/old.unit.test.ts", "// mirrors src/fleet/lease.zig\n"),
        }
        for rule, (path, text) in cases.items():
            with self.subTest(rule=rule):
                files = with_file(path, text)
                if rule == "cli-pulse-callers":
                    files["cli/src/lib/b.ts"] = "palette.pulse(c);\n"
                code, output = lint_tree(files)
                self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f": {rule}: ", output)

    def test_every_spelling_of_the_escape_character_is_a_raw_escape(self) -> None:
        for spelling in ("\\x1b", "\\x1B", "\\u001b", "\\u001B", "\\u{1b}", "\\u{001B}", "\\e", "\x1b"):
            with self.subTest(spelling=spelling):
                code, output = lint_tree(with_file("cli/src/lib/spinner.ts", f'const red = "{spelling}[31m";\n'))
                self.assertIn("cli/src/lib/spinner.ts:1: cli-raw-escape: ", output)

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

    def test_the_banner_carries_no_box_drawing_border(self) -> None:
        code, output = lint_tree(with_file(checker.BANNER, 'export const banner = "╭ agentsfleet ╮";\n'))
        self.assertIn(f"{checker.BANNER}:1: cli-retired-glyph: ", output)

    def test_a_citation_without_a_directory_is_out_of_reach(self) -> None:
        code, output = lint_tree(with_file("cli/test/old.unit.test.ts", "// once lived in lease.zig\n"))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)


if __name__ == "__main__":
    unittest.main()
