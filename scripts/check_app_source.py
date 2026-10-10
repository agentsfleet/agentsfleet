#!/usr/bin/env python3
"""Source rules for the dashboard (`ui/packages/app`).

Each rule is a convention the app keeps by declaring a thing once: an event
name in the analytics catalog, a timestamp through `Time`, a bearer only on
the server, an acceptance bundle in the shared seed. A finding is a second
spelling somewhere else. The JSX rules live in `check_app_jsx.py`.

    python3 scripts/check_app_source.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess
import sys

sys.dont_write_bytecode = True

import check_app_jsx as jsx
import check_source_lib as lib

APP = "ui/packages/app"
TS = (".ts", ".tsx")

# --- The acceptance suite ----------------------------------------------------
ACCEPTANCE = f"{APP}/tests/e2e/acceptance"
SHARED_SEED = f"{ACCEPTANCE}/fixtures/seed.ts"
# A spec-local bundle builder is how installs started failing with
# UZ-BUNDLE-001: the copies drifted from the importer's required keys.
LOCAL_BUILDER = re.compile(r"\bfunction (?:triggerMd|skillMd|emptyBodySkillMd)\b")
# A per-run template name mints a library row per run that nothing deletes.
INSTALL_CALL = re.compile(r"installViaUI\(\s*page,\s*(?P<template>[^,]+),")
CONSTANT_NAME = re.compile(r"^[A-Z][A-Z0-9_]*$")
# Unreachable while the Clerk testing proxy holds retried requests in flight.
NETWORK_IDLE = "networkidle"
JOURNEYS_PROJECT = "journeys"
EXECUTION_SPEC = "fleet-execution.spec.ts"
ACCEPTANCE_CONFIG = "playwright.acceptance.config.ts"
PLAYWRIGHT = "node_modules/.bin/playwright"

# --- The analytics catalog ---------------------------------------------------
EVENTS_MODULE = f"{APP}/lib/analytics/events.ts"
EVENTS_BLOCK = re.compile(r"export const EVENTS = \{(?P<body>.*?)\}", re.DOTALL)
EVENT_ENTRY = re.compile(r'^\s*\w+:\s*"(?P<name>[a-z_]+)"', re.MULTILINE)

# --- The browser holds no dashboard credential ------------------------------
CLI_HANDOFF = f"{APP}/app/cli-auth/[session_id]/page.tsx"
API_TEMPLATE_MINT = re.compile(r'template:\s*"api"')
USE_CLIENT = re.compile(r"""^\s*["']use client["']""", re.MULTILINE)
CLIENT_CREDENTIAL = {
    "a getToken() call": re.compile(r"\bgetToken\("),
    "an Authorization: Bearer header": re.compile(r"Authorization\s*:\s*.?Bearer"),
    "a token-typed prop": re.compile(r"(?:^|[^A-Za-z_.])token\s*[?:=](?!=)"),
}

# --- Timestamps render through Time -----------------------------------------
DATE_LABEL_HOME = f"{APP}/app/(dashboard)/settings/billing/lib/charges.ts"
RETIRED_FORMATTER = re.compile(r"\bformatDate\b")
LOCALE_DATE = re.compile(r"toLocale(?:Date|Time)?String|Intl\.RelativeTimeFormat")
INTL_DATE = re.compile(r"Intl\.DateTimeFormat")
TIMEZONE_READ = "resolvedOptions"
NON_PRODUCTION_DIRS = ("tests",)


def finding(root: Path, path: Path, text: str, offset: int, rule: str, message: str) -> lib.Finding:
    return lib.Finding(lib.relative(root, path), lib.line_of(text, offset), rule, message)


def production_files(root: Path) -> list[Path]:
    base = root / APP
    return [p for p in lib.source_files(base, TS) if p.relative_to(base).parts[0] not in NON_PRODUCTION_DIRS]


def check_acceptance_suite(root: Path) -> list[lib.Finding]:
    found = []
    for path in lib.source_files(root / ACCEPTANCE, (".ts",), include_tests=True):
        code = lib.blank_js_comments(path.read_text(encoding="utf-8"))
        builder = LOCAL_BUILDER.search(code)
        if builder and lib.relative(root, path) != SHARED_SEED:
            found.append(finding(root, path, code, builder.start(), "app-acceptance-builder", f"build bundles with {SHARED_SEED}, never a local copy"))
        for call in INSTALL_CALL.finditer(code):
            if not CONSTANT_NAME.match(call["template"].strip()):
                found.append(finding(root, path, code, call.start(), "app-acceptance-template", "name the installed template with a constant; a per-run name leaks a library row"))
        at = code.find(NETWORK_IDLE)
        if at != -1:
            found.append(finding(root, path, code, at, "app-acceptance-networkidle", f"never wait for {NETWORK_IDLE}; the Clerk testing proxy keeps requests in flight"))
    return found


def journey_findings(listing: str) -> list[lib.Finding]:
    """The execution journey must be among the journeys project's tests."""
    if f"[{JOURNEYS_PROJECT}] › {EXECUTION_SPEC}" in listing:
        return []
    return [lib.Finding(f"{APP}/{ACCEPTANCE_CONFIG}", 1, "app-journey-listed", f"the {JOURNEYS_PROJECT} project does not run {EXECUTION_SPEC}")]


def list_journeys(root: Path) -> str | None:
    """Playwright's listing of the journeys project, or None without Playwright."""
    playwright = root / APP / PLAYWRIGHT
    if not playwright.exists():
        return None
    listing = subprocess.run(
        [str(playwright), "test", "--list", f"--project={JOURNEYS_PROJECT}", "-c", ACCEPTANCE_CONFIG],
        cwd=root / APP, capture_output=True, text=True, check=False,
    )
    return listing.stdout


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


def check_client_credentials(root: Path) -> list[lib.Finding]:
    found = []
    for path in production_files(root):
        relative_path = lib.relative(root, path)
        if relative_path == CLI_HANDOFF:
            continue
        code = lib.blank_js_comments(path.read_text(encoding="utf-8"))
        mint = API_TEMPLATE_MINT.search(code)
        if mint:
            found.append(finding(root, path, code, mint.start(), "app-api-template", f"only {CLI_HANDOFF} mints the api-template token"))
        if not USE_CLIENT.search(code):
            continue
        for what, pattern in CLIENT_CREDENTIAL.items():
            for match in pattern.finditer(code):
                found.append(finding(root, path, code, match.start(), "app-client-credential", f"{what} in a client component would ship a bearer to the browser"))
    return found


def check_date_formatters(root: Path) -> list[lib.Finding]:
    found = []
    for path in production_files(root):
        code = lib.blank_js_comments(path.read_text(encoding="utf-8"))
        retired = RETIRED_FORMATTER.search(code)
        if retired:
            found.append(finding(root, path, code, retired.start(), "app-date-formatter", "formatDate was deleted; render through Time"))
        if lib.relative(root, path) == DATE_LABEL_HOME:
            continue
        for number, line in enumerate(code.splitlines(), start=1):
            if LOCALE_DATE.search(line) or (INTL_DATE.search(line) and TIMEZONE_READ not in line):
                found.append(lib.Finding(lib.relative(root, path), number, "app-date-formatter", "a bespoke date formatter; render through Time from @agentsfleet/design-system"))
    return found


RULES = (
    check_acceptance_suite, check_execution_journey, check_event_literals,
    check_client_credentials, check_date_formatters, jsx.check_button_variants, jsx.check_interface_fonts,
)


def lint(root: Path) -> list[lib.Finding]:
    return [f for rule in RULES for f in rule(root)]


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "app source", lint))
