#!/usr/bin/env python3
"""Each design-system rule refuses the regression it was written for.

    python3 scripts/check_design_system_source_test.py
"""

from __future__ import annotations

import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

import check_design_system_source as ds

OPTION_CARD = 'export const card = cn("rounded-md bg-surface-1", "data-[state=checked]:border-pulse");\n'
USAGE_BAR = 'export const bar = "h-2 w-full bg-surface-2"; // a comment may say #123 or items[0]\n'
TOKENS_CSS = """\
:root {
  --bg: #131d21;
  --surface-deep: #0e171b;
  --surface-1: #202f35;
  --surface-2: #2a3b42;
  --surface-3: #31434c;
  --text: #f1f5f5;
  --text-muted: #c6d2d5;
  --text-subtle: #acbec4;
  --pulse: #5eead4;
  --success: #4ade80;
  --warn: #fbbf24;
  --error: #fca5a5;
  --info: var(--text-muted);
}

[data-theme="light"] {
  --bg: #f7faf9;
  --surface-deep: #e8efed;
  --surface-1: #fff;
  --surface-2: #edf3f1;
  --surface-3: #dfe9e5;
  --text: #14251f;
  --text-muted: #435d52;
  --text-subtle: #4e655b;
  --pulse: #06695e;
  --success: rgb(7, 104, 61);
  --warn: #754c00;
  --error: #a82d36;
  --info: #2455ac;
}
"""
UTILS_TS = "export function cn(...inputs: string[]) { return inputs.join(' '); }\n"


def clean_tree() -> dict[str, str]:
    return {
        ds.TOKEN_ONLY_COMPONENTS[0]: OPTION_CARD,
        ds.TOKEN_ONLY_COMPONENTS[1]: USAGE_BAR,
        ds.TOKENS_CSS: TOKENS_CSS,
        ds.CN_HOME: UTILS_TS,
        f"{ds.PACKAGES}/app/lib/re-export.ts": 'export { cn } from "@agentsfleet/design-system";\n',
    }


def lint_tree(module, files: dict[str, str]) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for relative_path, text in files.items():
            (root / relative_path).parent.mkdir(parents=True, exist_ok=True)
            (root / relative_path).write_text(text, encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = module.lib.run([module.lib.ROOT_FLAG, str(root)], "ui", module.lint)
        return code, out.getvalue()


def seeded(path: str, old: str, new: str) -> dict[str, str]:
    files = clean_tree()
    assert old in files[path], (path, old)
    files[path] = files[path].replace(old, new)
    return files


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(ds, clean_tree())
        self.assertEqual(ds.lib.EXIT_CLEAN, code, output)

    def test_moved_rules_reject_seeded_violations(self) -> None:
        cases = {
            "ds-mapped-tokens": (ds, seeded(ds.TOKEN_ONLY_COMPONENTS[1], "w-full", "w-[42px]")),
            "ds-contrast": (ds, seeded(ds.TOKENS_CSS, "--text-subtle: #4e655b;", "--text-subtle: #c0c8c4;")),
            "ds-single-cn": (ds, {**clean_tree(), f"{ds.PACKAGES}/app/lib/cn.ts": "export const cn = (a: string) => a;\n"}),
        }
        for rule, (module, files) in cases.items():
            with self.subTest(rule=rule):
                code, output = lint_tree(module, files)
                self.assertEqual(module.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f": {rule}: ", output)

    def test_a_hex_colour_in_a_class_string_is_refused_and_a_variant_bracket_is_not(self) -> None:
        code, output = lint_tree(ds, seeded(ds.TOKEN_ONLY_COMPONENTS[0], "bg-surface-1", "text-[#fff] bg-surface-1"))
        self.assertIn("`text-[#fff]` is an arbitrary-value utility", output)
        self.assertIn("`#fff` is a hex colour", output)

    def test_a_colour_the_check_cannot_evaluate_is_named_rather_than_skipped(self) -> None:
        code, output = lint_tree(ds, seeded(ds.TOKENS_CSS, "--warn: #754c00;", "--warn: oklch(0.5 0.1 80);"))
        self.assertIn("light: --warn is missing or not a colour this check can evaluate", output)

    def test_a_lost_cn_is_named(self) -> None:
        files = {k: v for k, v in clean_tree().items() if k != ds.CN_HOME}
        code, output = lint_tree(ds, files)
        self.assertIn("the design system's cn is missing", output)


if __name__ == "__main__":
    unittest.main()
