#!/usr/bin/env python3
"""Parity self-tests: the README coverage badges against the uploads that feed them.

A coverage badge is the one number a stranger reads before the code, and it has
two ways to lie. It renders `unknown` when the README names a flag no workflow
uploads — worse than no badge, because it looks like a broken project rather
than a missing step. And it renders a number the gate never enforced when the
upload hands Codecov the per-component kcov reports instead of the merged union:
Codecov would build its own union over its own denominator, counting the harness
files and the inline `test {}` blocks `check_zig_coverage.py` excludes, and the
badge would read roughly two points above the figure that gated the branch.

So the README, the upload steps, and the merged report path are checked against
each other here. The package flags upload from `.github/workflows/test.yml`,
the zig flags from `.github/workflows/test-integration.yml` — where the grade
job and the merged report live — and the paths from `make/test.mk`; this module
owns no numbers of its own.

Run: python3 -m unittest discover -s scripts -t scripts -p '*_test.py'
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path
from urllib.parse import parse_qs, urlsplit

REPO_ROOT = Path(__file__).resolve().parents[1]
README = REPO_ROOT / "README.md"
# Both workflows that upload to Codecov: the package lanes stayed in test.yml;
# the three zig flags moved with the coverage grade into test-integration.yml.
WORKFLOWS = (
    REPO_ROOT / ".github" / "workflows" / "test.yml",
    REPO_ROOT / ".github" / "workflows" / "test-integration.yml",
)
MAKE_TEST = REPO_ROOT / "make" / "test.mk"

CODECOV_ACTION = "codecov/codecov-action@"
# The merged report is the only Zig artefact Codecov may see. Every Zig flag is
# enumerated, so a new one cannot inherit the assertion by accident — it has to
# be added here, which is the moment to ask whether it names the union too.
# The three publish the SAME union scoped by the `paths` filters in
# codecov.yml, matching the per-folder floors in make/test.mk.
ZIG_FLAGS = frozenset({"zig-agentsfleetd", "zig-runner", "zig-lib"})
ZIG_FLAG_PREFIX = "zig"
MERGED_REPORT_NAME = "merged/cobertura.xml"

# `[![zig coverage](https://img.shields.io/codecov/...)](https://codecov.io/...)`
README_CODECOV_BADGE = re.compile(
    r"\[!\[(?P<alt>[^\]]+)\]\((?P<image>https://img\.shields\.io/codecov/[^)]+)\)\]"
    r"\((?P<link>https://codecov\.io/[^)]+)\)"
)
# `- name: Upload zig coverage to Codecov` — the line every workflow step opens with.
WORKFLOW_STEP_START = re.compile(r"^\s*-\s+name:\s*(?P<name>.+?)\s*$")
# `files: coverage/zig/merged/cobertura.xml` inside a step's `with:` mapping.
WORKFLOW_SETTING = re.compile(r"^\s*(?P<key>[a-z_]+):\s*(?P<value>.*?)\s*$")
# `ZIG_COVERAGE_DIR ?= $(CURDIR)/coverage/zig`
MAKE_COVERAGE_DIR = re.compile(r"^ZIG_COVERAGE_DIR\s*\?=\s*(?P<value>.*)$", re.MULTILINE)
MAKE_CURDIR = "$(CURDIR)/"


def zig_coverage_dir() -> str:
    """The repository-relative Zig coverage directory, read from its one make definition."""
    match = MAKE_COVERAGE_DIR.search(MAKE_TEST.read_text(encoding="utf-8"))
    if match is None:
        raise AssertionError(f"ZIG_COVERAGE_DIR is not defined in {MAKE_TEST}")
    value = match.group("value").strip()
    if not value.startswith(MAKE_CURDIR):
        raise AssertionError(f"ZIG_COVERAGE_DIR is not repository-relative: {value}")
    return value[len(MAKE_CURDIR) :]


def readme_badges() -> dict[str, dict[str, str]]:
    """Every Codecov badge in the README, keyed by the flag its image requests."""
    badges: dict[str, dict[str, str]] = {}
    for match in README_CODECOV_BADGE.finditer(README.read_text(encoding="utf-8")):
        image = parse_qs(urlsplit(match.group("image")).query)
        flags = image.get("flag", [])
        if len(flags) != 1:
            raise AssertionError(f"badge image names {len(flags)} flags: {match.group('image')}")
        badges[flags[0]] = {
            "alt": match.group("alt"),
            "label": (image.get("label") or [""])[0],
            "link": match.group("link"),
        }
    return badges


def codecov_uploads() -> list[dict[str, str]]:
    """Every `codecov-action` step's settings, in workflow order, with its step name."""
    uploads: list[dict[str, str]] = []
    for workflow in WORKFLOWS:
        uploads.extend(workflow_codecov_uploads(workflow))
    return uploads


def workflow_codecov_uploads(workflow: Path) -> list[dict[str, str]]:
    uploads: list[dict[str, str]] = []
    current: dict[str, str] | None = None
    for line in workflow.read_text(encoding="utf-8").splitlines():
        step = WORKFLOW_STEP_START.match(line)
        if step is not None:
            if current is not None and CODECOV_ACTION in current.get("uses", ""):
                uploads.append(current)
            current = {"name": step.group("name")}
            continue
        if current is None:
            continue
        setting = WORKFLOW_SETTING.match(line)
        if setting is not None and setting.group("value"):
            current.setdefault(setting.group("key"), setting.group("value"))
    if current is not None and CODECOV_ACTION in current.get("uses", ""):
        uploads.append(current)
    return uploads


class ReadmeBadgeRow(unittest.TestCase):
    def test_readme_badge_row_is_well_formed(self) -> None:
        """Each badge asks for one flag, labels itself with it, and links to it."""
        badges = readme_badges()
        self.assertTrue(badges, "the README carries no Codecov badge")
        for flag, badge in badges.items():
            self.assertEqual(
                flag,
                badge["label"],
                f"badge for flag {flag!r} is labelled {badge['label']!r} — a reader"
                " cannot tell which surface the number belongs to",
            )
            self.assertIn(
                flag,
                parse_qs(urlsplit(badge["link"]).query).get("flags[0]", []),
                f"badge for flag {flag!r} links to a different flag's page",
            )

    def test_every_readme_flag_has_an_upload(self) -> None:
        """No badge can render `unknown`, and no upload is published without a badge."""
        badge_flags = set(readme_badges())
        upload_flags = {upload["flags"] for upload in codecov_uploads() if "flags" in upload}
        self.assertEqual(
            badge_flags - upload_flags,
            set(),
            "the README names flags nothing uploads — these badges render `unknown`",
        )
        self.assertEqual(
            upload_flags - badge_flags,
            set(),
            "coverage is uploaded under flags the README never shows",
        )

    def test_every_zig_upload_names_the_merged_report(self) -> None:
        """Codecov sees the union this gate graded, never the per-component reports."""
        uploads = [
            upload for upload in codecov_uploads() if upload.get("flags") in ZIG_FLAGS
        ]
        self.assertEqual(
            {upload["flags"] for upload in uploads},
            set(ZIG_FLAGS),
            f"expected one upload per Zig flag ({sorted(ZIG_FLAGS)})",
        )
        expected = f"{zig_coverage_dir()}/{MERGED_REPORT_NAME}"
        for upload in uploads:
            self.assertEqual(
                upload.get("files"),
                expected,
                f"{upload['flags']!r} must name the merged report; a per-component"
                " report lets Codecov build its own union over a denominator this"
                " gate excludes",
            )

    def test_no_zig_flag_escapes_the_enumeration(self) -> None:
        """A fourth Zig folder must join ZIG_FLAGS, not publish ungraded.

        The assertion above only reaches the flags it already knows. Without
        this, adding `zig-<folder>` to the workflow and the README would pass
        every check here while nothing verified it names the union.
        """
        published = {
            upload["flags"]
            for upload in codecov_uploads()
            if upload.get("flags", "").startswith(ZIG_FLAG_PREFIX)
        }
        self.assertEqual(
            published - set(ZIG_FLAGS),
            set(),
            "a Zig flag is uploaded that ZIG_FLAGS does not enumerate — add it"
            " there so its report path is checked too",
        )

    def test_every_upload_disables_report_search(self) -> None:
        """Explicit files only — discovery would find the reports the union excludes."""
        for upload in codecov_uploads():
            self.assertEqual(
                upload.get("disable_search"),
                "true",
                f"{upload['name']!r} lets Codecov search for reports; it would find"
                " the per-component kcov output beside the merged union",
            )
            self.assertTrue(
                upload.get("files"),
                f"{upload['name']!r} names no report file",
            )


if __name__ == "__main__":
    unittest.main()
