#!/usr/bin/env python3
"""Copy policy for the marketing site (`ui/packages/website/src`).

The positioning is a decision the copy carries: no retired pull-request
validator framing, no quantitative or autonomous-merge claim nobody has
validated, and the product noun is Fleet. A finding is a line of source that
says otherwise, comments included, since a comment is copied into copy.

    python3 scripts/check_website_copy.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

WEBSITE_SRC = "ui/packages/website/src"
SOURCE_SUFFIXES = (".ts", ".tsx", ".js", ".jsx")

# The v1 pull-request-validator positioning, retired with the product it sold.
RETIRED_FRAMING = (
    "AI-generated PRs",
    "Automated PR delivery",
    "babysit",
    "Connect GitHub, automate PRs",
    "Validated PR delivery",
    "Run quality scoring",
    "run quality scoring",
    "Review a validated PR",
    "Queue work",
    "queued engineering work",
    "Validation before review",
)

# Claims no measurement backs, matched case-insensitively.
UNVALIDATED_CLAIMS = (
    "zero tickets",
    "autonomous merge",
    "autonomous deploy",
    "40%",
    "hour response time",
    "ticket latency",
)

# The retired brand and entity noun, assembled so this file does not spell
# either and trip a repository-wide brand search.
RETIRED_NOUN = "zom" + "bie"
RETIRED_BRAND = re.compile(rf"\b(?:use)?{RETIRED_NOUN}s?\b", re.IGNORECASE)


def lint(root: Path) -> list[lib.Finding]:
    found = []
    sources = lib.source_files(root / WEBSITE_SRC, SOURCE_SUFFIXES)
    if not sources:
        return [lib.Finding(WEBSITE_SRC, 1, "web-copy", "no website source was read")]
    for path in sources:
        relative_path = lib.relative(root, path)
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            lowered = line.lower()
            for phrase in RETIRED_FRAMING:
                if phrase in line:
                    found.append(lib.Finding(relative_path, number, "web-retired-framing", f"`{phrase}` is the retired pull-request-validator positioning"))
            for claim in UNVALIDATED_CLAIMS:
                if claim in lowered:
                    found.append(lib.Finding(relative_path, number, "web-unvalidated-claim", f"`{claim}` is a claim no measurement backs"))
            if RETIRED_BRAND.search(line):
                found.append(lib.Finding(relative_path, number, "web-retired-noun", "the product noun is Fleet"))
    return found


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "website copy", lint))
