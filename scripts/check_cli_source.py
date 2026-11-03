#!/usr/bin/env python3
"""Source rules for the agentsfleet CLI (`cli/`).

The rule is a convention the CLI keeps by declaring a thing in one place: an
environment-variable name in `constants/env.ts`. A finding is a second
spelling somewhere else. It stays here because the names it bans are read
from that module; the single-file rules (escapes in the palette, detail
tables) are ast-grep rules under .ast-grep/rules/cli/, run by
`make lint-source`.

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
ENV_MODULE = "cli/src/constants/env.ts"
ENV_DECLARATION = re.compile(r'export const \w+ = "(?P<name>[A-Z][A-Z0-9_]+)"')


def finding(root: Path, path: Path, text: str, offset: int, rule: str, message: str) -> lib.Finding:
    return lib.Finding(lib.relative(root, path), lib.line_of(text, offset), rule, message)


def env_names(root: Path) -> list[str]:
    module = root / ENV_MODULE
    return ENV_DECLARATION.findall(module.read_text(encoding="utf-8")) if module.is_file() else []


def check_env_spelling(root: Path, sources: dict[Path, str]) -> list[lib.Finding]:
    names = env_names(root)
    if not names:
        return [lib.Finding(ENV_MODULE, 1, "cli-env-spelling", "no environment-variable name was declared")]
    alternatives = "|".join(re.escape(name) for name in names)
    spellings = re.compile(rf"([\"'`])(?:{alternatives})\1|\.(?:{alternatives})\b")
    found = []
    for path, code in sources.items():
        if lib.relative(root, path) == ENV_MODULE:
            continue
        for match in spellings.finditer(code):
            found.append(finding(root, path, code, match.start(), "cli-env-spelling", f"spell `{match.group(0).strip('.')}` through {ENV_MODULE}"))
    return found


def lint(root: Path) -> list[lib.Finding]:
    paths = lib.source_files(root / CLI_SRC, (".ts", ".js"))
    code = {path: lib.blank_js_comments(path.read_text(encoding="utf-8")) for path in paths}
    return check_env_spelling(root, code)


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "cli source", lint))
