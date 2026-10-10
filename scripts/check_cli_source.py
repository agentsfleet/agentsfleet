#!/usr/bin/env python3
"""Source rules for the agentsfleet CLI (`cli/`).

Each rule is a convention the CLI keeps by declaring a thing in one place:
an environment-variable name in `constants/env.ts`, a terminal escape in
`output/palette.ts`, a detail table in the three modules that render one. A
finding is a second spelling somewhere else.

    python3 scripts/check_cli_source.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

CLI_SRC = "cli/src"
CLI_TEST = "cli/test"
ENV_MODULE = "cli/src/constants/env.ts"
PALETTE = "cli/src/output/palette.ts"
BANNER = "cli/src/program/banner.ts"
COMMANDS = "cli/src/commands"
ENV_DECLARATION = re.compile(r'export const \w+ = "(?P<name>[A-Z][A-Z0-9_]+)"')

# A raw escape spelled any of the ways a TypeScript source can: the \u001b and
# \x1b escapes, the \e some editors paste, and the control character itself.
RAW_ESCAPE = re.compile(r"\\u001b\[|\\x1b\[|\x1b\[")
COLOR_256 = re.compile(r"38;5;\d+")
PULSE_CALL = re.compile(r"\bpalette\.pulse\b")
PULSE_CALLERS_MAX = 3
# The decorative glyphs the design system retired from CLI output.
RETIRED_GLYPHS = {"\U0001F9DF": "agent face", "\U0001F389": "party"}
BOX_DRAWING = ("╭", "╮", "╯", "╰", "│")
# A two-column label/value table for ONE resource is a different shape from a
# list of entities, and these are the commands that render one; every entity
# list goes through printEntityTable so its columns come from one order.
DETAIL_TABLE_MODULES = frozenset({"api_key.ts", "connector.ts", "tenant.ts"})
DETAIL_TABLE_CALL = "output.printTable("

# A path a comment cites: at least one directory, then a file with an extension.
CITED_PATH = re.compile(r"(?<![\w./:-])(?:[\w-]+/)+[\w.-]*\w(?P<extension>\.[a-z]{1,4})(?![\w/])")
COMMENT_LINE = re.compile(r"^\s*(?://|/\*|\*)")


def finding(root: Path, path: Path, text: str, offset: int, rule: str, message: str) -> lib.Finding:
    return lib.Finding(lib.relative(root, path), lib.line_of(text, offset), rule, message)


def env_names(root: Path) -> list[str]:
    module = root / ENV_MODULE
    return ENV_DECLARATION.findall(module.read_text(encoding="utf-8")) if module.is_file() else []


def check_env_spelling(root: Path, sources: dict[Path, str]) -> list[lib.Finding]:
    names = env_names(root)
    if not names:
        return [lib.Finding(ENV_MODULE, 1, "cli-env-spelling", "no environment-variable name was declared")]
    spellings = re.compile("|".join(
        rf"([\"'`]){re.escape(name)}\1|\.{re.escape(name)}\b" for name in names
    ))
    found = []
    for path, code in sources.items():
        if lib.relative(root, path) == ENV_MODULE:
            continue
        for match in spellings.finditer(code):
            found.append(finding(root, path, code, match.start(), "cli-env-spelling", f"spell `{match.group(0).strip('.')}` through {ENV_MODULE}"))
    return found


def check_output_currency(root: Path, sources: dict[Path, str]) -> list[lib.Finding]:
    found = []
    pulse_callers = []
    for path, text in sources.items():
        relative_path = lib.relative(root, path)
        if PULSE_CALL.search(text):
            pulse_callers.append(relative_path)
        if relative_path != PALETTE:
            for pattern, what in ((RAW_ESCAPE, "a raw terminal escape"), (COLOR_256, "a 256-color code")):
                for match in pattern.finditer(text):
                    found.append(finding(root, path, text, match.start(), "cli-raw-escape", f"{what} belongs in {PALETTE}, which honours NO_COLOR"))
        for glyph, name in RETIRED_GLYPHS.items():
            at = text.find(glyph)
            if at != -1:
                found.append(finding(root, path, text, at, "cli-retired-glyph", f"the {name} emoji was retired from CLI output"))
        if relative_path == BANNER:
            for glyph in BOX_DRAWING:
                at = text.find(glyph)
                if at != -1:
                    found.append(finding(root, path, text, at, "cli-retired-glyph", "the banner carries no box-drawing border"))
    if len(pulse_callers) > PULSE_CALLERS_MAX:
        found.append(lib.Finding(pulse_callers[-1], 1, "cli-pulse-callers", f"palette.pulse is called from {len(pulse_callers)} files (at most {PULSE_CALLERS_MAX}): {', '.join(pulse_callers)}"))
    return found


def check_detail_tables(root: Path) -> list[lib.Finding]:
    found = []
    for path in sorted((root / COMMANDS).glob("*.ts")):
        text = path.read_text(encoding="utf-8")
        at = text.find(DETAIL_TABLE_CALL)
        if at != -1 and path.name not in DETAIL_TABLE_MODULES:
            found.append(finding(root, path, text, at, "cli-detail-table", "an entity list renders through printEntityTable, not its own columns"))
    return found


def check_cited_paths(root: Path) -> list[lib.Finding]:
    tracked = {Path(path).suffix for path in lib.tracked_files(root)}
    found = []
    for base in (CLI_SRC, CLI_TEST):
        for path in lib.source_files(root / base, (".ts",), include_tests=True):
            lines = path.read_text(encoding="utf-8").splitlines()
            for number, line in enumerate(lines, start=1):
                if not COMMENT_LINE.match(line):
                    continue
                for match in CITED_PATH.finditer(line):
                    if match["extension"] not in tracked:
                        found.append(lib.Finding(lib.relative(root, path), number, "cli-retired-citation", f"`{match.group(0)}` cites a file in a language the repository no longer has"))
    return found


def lint(root: Path) -> list[lib.Finding]:
    paths = lib.source_files(root / CLI_SRC, (".ts", ".js"))
    raw = {path: path.read_text(encoding="utf-8") for path in paths}
    code = {path: lib.blank_js_comments(text) for path, text in raw.items()}
    return (
        check_env_spelling(root, code)
        + check_output_currency(root, raw)
        + check_detail_tables(root)
        + check_cited_paths(root)
    )


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "cli source", lint))
