#!/usr/bin/env python3
"""Parity self-tests: the coverage architecture doc against the gate it describes.

`docs/architecture/testing.md` is canonical for this lane, and it went stale the
only way a doc can — quietly. It published a 89% floor while the gate enforced a
different number, named `ZIG_COVERAGE_MIN_LINES` after that variable had been
renamed, and claimed per-folder floors could not be enforced in Continuous
Integration (CI) long after they were. Every one of those read as authoritative.

So the numbers live in `make/test.mk` alone, and these tests fail when the doc
disagrees with it. A doc nothing checks is a doc that describes the past.

Run: python3 -m unittest discover -s scripts -t scripts -p '*_test.py'
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
MAKE_TEST = REPO_ROOT / "make" / "test.mk"
MAKE_TEST_UNIT = REPO_ROOT / "make" / "test-unit.mk"
ARCHITECTURE_DOC = REPO_ROOT / "docs" / "architecture" / "testing.md"

MERGED_SCOPE = "merged"
# Variables retired by this lane. Naming one in the doc sends a reader to a
# definition site that no longer exists.
RETIRED_MAKE_VARIABLES = ("ZIG_COVERAGE_MIN_LINES",)

# `NAME ?= value`, the form every coverage variable in make/test.mk uses.
MAKE_ASSIGNMENT = re.compile(r"^([A-Z_]+)\s*\?=\s*(.*)$", re.MULTILINE)
# `| merged | 88 | 95 |`, with or without the backticks a folder name carries.
# Digits belong in every component pattern here: `s3` is one.
DOC_TABLE_ROW = re.compile(r"^\|\s*`?([a-z0-9_]+)`?\s*\|\s*(\d+)\s*\|\s*(\d+)\s*\|\s*$", re.MULTILINE)
# `- `lifecycle` — the boot to SIGTERM to drain proof, ...`
DOC_COMPONENT_BULLET = re.compile(r"^-\s+`([a-z0-9_]+)`\s+—", re.MULTILINE)
# `components="agentsfleetd:agentsfleetd-tests runner:agentsfleet-runner-tests"`
MAKE_COMPONENT_LIST = re.compile(r'components="([^"]*)"')
# The two components announced by name rather than iterated: they run serially.
MAKE_LITERAL_COMPONENT = re.compile(r"kcov component=([a-z0-9_]+) binary=")


def make_variables() -> dict[str, str]:
    """Every `NAME ?= value` in make/test.mk, first definition winning."""
    text = MAKE_TEST.read_text(encoding="utf-8")
    values: dict[str, str] = {}
    for name, value in MAKE_ASSIGNMENT.findall(text):
        values.setdefault(name, value.strip())
    return values


def make_pairs(value: str) -> dict[str, int]:
    """`agentsfleetd=87 runner=91` -> {'agentsfleetd': 87, 'runner': 91}."""
    pairs: dict[str, int] = {}
    for token in value.split():
        scope, _, number = token.partition("=")
        pairs[scope] = int(number)
    return pairs


def gate_floors_and_targets() -> dict[str, tuple[int, int]]:
    """The enforced floor and published target per scope, from make/test.mk."""
    variables = make_variables()
    floors = make_pairs(variables["ZIG_COVERAGE_FOLDER_FLOORS"])
    targets = make_pairs(variables["ZIG_COVERAGE_FOLDER_TARGETS"])
    scopes = {
        MERGED_SCOPE: (
            int(variables["ZIG_COVERAGE_MIN_PCT"]),
            int(variables["ZIG_COVERAGE_TARGET_PCT"]),
        )
    }
    for scope, floor in floors.items():
        scopes[scope] = (floor, targets[scope])
    return scopes


def documented_floors_and_targets() -> dict[str, tuple[int, int]]:
    """The same mapping as the doc's floors table states it."""
    text = ARCHITECTURE_DOC.read_text(encoding="utf-8")
    return {
        scope: (int(floor), int(target))
        for scope, floor, target in DOC_TABLE_ROW.findall(text)
    }


def gate_components() -> set[str]:
    """Every component the coverage recipe measures, iterated or named."""
    text = MAKE_TEST_UNIT.read_text(encoding="utf-8")
    components: set[str] = set()
    for group in MAKE_COMPONENT_LIST.findall(text):
        for token in group.split():
            name, separator, _ = token.partition(":")
            if separator:
                components.add(name)
    components.update(MAKE_LITERAL_COMPONENT.findall(text))
    return components


def documented_components() -> set[str]:
    return set(DOC_COMPONENT_BULLET.findall(ARCHITECTURE_DOC.read_text(encoding="utf-8")))


class ArchitectureDocMatchesTheGate(unittest.TestCase):
    def test_architecture_doc_matches_gate_values(self) -> None:
        self.assertEqual(
            documented_floors_and_targets(),
            gate_floors_and_targets(),
            "the floors table in docs/architecture/testing.md disagrees with make/test.mk — "
            "raise both in the same commit as the tests that clear the new value",
        )

    def test_every_product_scope_is_documented(self) -> None:
        self.assertIn(MERGED_SCOPE, documented_floors_and_targets())
        for scope in gate_floors_and_targets():
            self.assertIn(
                scope,
                documented_floors_and_targets(),
                f"scope {scope!r} is enforced by the gate and absent from the doc",
            )

    def test_architecture_doc_lists_every_measured_component(self) -> None:
        self.assertEqual(
            documented_components(),
            gate_components(),
            "the component list in docs/architecture/testing.md disagrees with the "
            "components make/test-unit.mk runs under kcov",
        )

    def test_architecture_doc_names_no_retired_variable(self) -> None:
        text = ARCHITECTURE_DOC.read_text(encoding="utf-8")
        for retired in RETIRED_MAKE_VARIABLES:
            self.assertNotIn(
                retired,
                text,
                f"{retired} no longer exists; the doc sends a reader to nothing",
            )

    def test_the_doc_carries_no_conflict_marker(self) -> None:
        # One shipped to the default branch appended to the end of a sentence,
        # where a line-anchored grep could not see it.
        text = ARCHITECTURE_DOC.read_text(encoding="utf-8")
        for marker in ("<<<<<<<", ">>>>>>>", "\n=======\n"):
            self.assertNotIn(marker, text, "an unresolved merge conflict is in the doc")


if __name__ == "__main__":
    unittest.main()
