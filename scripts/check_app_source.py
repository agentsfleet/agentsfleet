#!/usr/bin/env python3
"""Source rules for the dashboard (`ui/packages/app`).

Each rule is a convention the app keeps by declaring a thing once: an event
name in the analytics catalog, the execution journey in the journeys project.
A finding is a second spelling, or the journey gone missing.

The rules that are one pattern in one file (no networkidle wait, Time for
dates, no browser bearer, one api-template mint) are ast-grep rules under
.ast-grep/rules/app/, run by `make lint-source`. These two stay here because
each compares the tree with something outside the file it reads: the catalog
module, or Playwright's own listing.

    python3 scripts/check_app_source.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib

APP = "ui/packages/app"
TS = (".ts", ".tsx")

# --- The acceptance suite ----------------------------------------------------
JOURNEYS_PROJECT = "journeys"
EXECUTION_SPEC = "fleet-execution.spec.ts"
ACCEPTANCE_CONFIG = "playwright.acceptance.config.ts"
PLAYWRIGHT = "node_modules/.bin/playwright"
# The specs throw at import when NEXT_PUBLIC_API_URL is unset, and the lint job
# sets no backend. Listing never connects, so a reserved, unroutable origin
# lets the specs load without pointing them at anything real.
LISTING_API_URL = "https://listing.invalid"

# --- The analytics catalog ---------------------------------------------------
EVENTS_MODULE = f"{APP}/lib/analytics/events.ts"
EVENTS_BLOCK = re.compile(r"export const EVENTS = \{(?P<body>.*?)\}", re.DOTALL)
EVENT_ENTRY = re.compile(r'^\s*\w+:\s*"(?P<name>[a-z_]+)"', re.MULTILINE)


def finding(root: Path, path: Path, text: str, offset: int, rule: str, message: str) -> lib.Finding:
    return lib.Finding(lib.relative(root, path), lib.line_of(text, offset), rule, message)


def journey_findings(listing: subprocess.CompletedProcess[str]) -> list[lib.Finding]:
    """The execution journey must be among the journeys project's tests."""
    where = f"{APP}/{ACCEPTANCE_CONFIG}"
    if listing.returncode != 0:
        error = listing.stderr.strip() or listing.stdout.strip() or f"exit {listing.returncode}"
        return [lib.Finding(where, 1, "app-journey-listed", f"Playwright could not list the {JOURNEYS_PROJECT} project: {error}")]
    if f"[{JOURNEYS_PROJECT}] › {EXECUTION_SPEC}" in listing.stdout:
        return []
    return [lib.Finding(where, 1, "app-journey-listed", f"the {JOURNEYS_PROJECT} project does not run {EXECUTION_SPEC}")]


def list_journeys(root: Path) -> subprocess.CompletedProcess[str] | None:
    """Playwright's listing of the journeys project, or None without Playwright."""
    playwright = root / APP / PLAYWRIGHT
    if not playwright.exists():
        return None
    return subprocess.run(
        [str(playwright), "test", "--list", "--reporter=list", f"--project={JOURNEYS_PROJECT}", "-c", ACCEPTANCE_CONFIG],
        cwd=root / APP, capture_output=True, text=True, check=False,
        env={**os.environ, "NEXT_PUBLIC_API_URL": LISTING_API_URL},
    )


def check_execution_journey(root: Path) -> list[lib.Finding]:
    listing = list_journeys(root)
    if listing is None:
        return [lib.Finding(f"{APP}/{PLAYWRIGHT}", 1, "app-journey-listed", "Playwright is not installed; run bun install in the app")]
    return journey_findings(listing)


def event_names(root: Path) -> list[str]:
    module = root / EVENTS_MODULE
    block = EVENTS_BLOCK.search(module.read_text(encoding="utf-8")) if module.is_file() else None
    return EVENT_ENTRY.findall(block["body"]) if block else []


def check_event_literals(root: Path) -> list[lib.Finding]:
    names = event_names(root)
    if not names:
        return [lib.Finding(EVENTS_MODULE, 1, "app-event-literal", "no event name was read from the catalog")]
    literal = re.compile(r"""(["'`])(?:%s)\1""" % "|".join(map(re.escape, names)))
    found = []
    for path in lib.source_files(root / APP, TS, include_tests=True):
        if lib.relative(root, path) == EVENTS_MODULE:
            continue
        code = lib.blank_js_comments(path.read_text(encoding="utf-8"))
        for match in literal.finditer(code):
            found.append(finding(root, path, code, match.start(), "app-event-literal", f"reference {match.group(0)} through EVENTS, the one catalog"))
    return found


RULES =(check_execution_journey, check_event_literals)


def lint(root: Path) -> list[lib.Finding]:
    return [f for rule in RULES for f in rule(root)]


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "app source", lint))
