#!/usr/bin/env python3
"""Classify every unhit line in the merged Zig coverage report by mechanism.

A coverage percentage says how much ran. It does not say what kind of code did
not, and those kinds are not interchangeable: an `errdefer` rung that never ran
is a leak nobody has disproved, while a closing brace that never ran is a
report artefact worth no test at all. Grading a sweep on the percentage alone
therefore rewards colouring whichever lines are cheapest to reach.

This groups each unhit line by the mechanism a test would have to use to reach
it, so the sweep can be worked and graded class by class:

  errdefer          cleanup that runs only when a later allocation fails
  failure-response  the arm that answers a caller when the request failed
  failure-log       the operator-facing log line beside such an arm
  error-return      `return error.x`, `catch return`, `orelse return`
  brace             braces, blank lines, and other syntax carrying no behaviour
  other             ordinary statements and branches

Two things stop the classes from lying. A line inside a multi-line call is
attributed to the call's first line, so the fields of a log call count as one
log line rather than five `other` lines. And inline `test {}` bodies are
dropped through the same helper the coverage gate uses, so a class can never be
emptied by writing more test code inside a product file.
"""

from __future__ import annotations

import argparse
import collections
import re
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path

from check_zig_coverage_floors import inline_test_lines

CLASS_ERRDEFER = "errdefer"
CLASS_FAILURE_RESPONSE = "failure-response"
CLASS_FAILURE_LOG = "failure-log"
CLASS_ERROR_RETURN = "error-return"
CLASS_BRACE = "brace"
CLASS_OTHER = "other"

# Ordered by the sweep's section order, which is also the order a reader wants
# a report in: the classes that carry risk first, the inert one last.
CLASS_NAMES = (
    CLASS_ERRDEFER,
    CLASS_FAILURE_RESPONSE,
    CLASS_FAILURE_LOG,
    CLASS_ERROR_RETURN,
    CLASS_OTHER,
    CLASS_BRACE,
)

DEFAULT_REPORT = Path("coverage/zig/merged/cobertura.xml")

# `errdefer expr;` and `errdefer {` both open the class; the block form also
# claims the lines beneath it, which the block tracker below handles.
ERRDEFER_HEAD = re.compile(r"^\s*errdefer\b")

# The error-return family. `return err;` is the propagating arm of a `catch
# |err|`, so it belongs here rather than with ordinary returns.
ERROR_RETURN = re.compile(
    r"\breturn\s+error\.|"
    r"\breturn\s+err\s*;|"
    r"\bcatch\s+return\b|"
    r"\borelse\s+return\b|"
    r"\breturn\s+[A-Za-z_]\w*Error\."
)

# Helpers that write a failure to the caller. Enumerated rather than matched on
# a bare `fail` substring: a substring rule would also claim `failed_at` and
# every identifier that merely contains the word.
FAILURE_RESPONSE = re.compile(
    r"\.fail\s*\(|"
    r"\.internalDbError\s*\(|"
    r"\.internalDbUnavailable\s*\(|"
    r"\.internalOperationError\s*\(|"
    r"\.errorResponse\s*\("
)

# Only `err` and `warn`. `debug` and `info` fire on healthy paths too, so
# counting them here would inflate the class with lines no failure reaches.
FAILURE_LOG = re.compile(r"\blog\.(?:err|warn)\s*\(")

# A line whose entire content is punctuation, or nothing at all.
BRACE_ONLY = re.compile(r"^[\s{}()\[\];,]*$")


@dataclass(frozen=True)
class UnhitLine:
    """One never-executed line, with the class that names how to reach it."""

    path: str
    number: int
    text: str
    kind: str


class UsageError(ValueError):
    """An argument the caller got wrong, not a classification failure."""


def scan_deltas(text: str) -> tuple[int, int]:
    """Net brace and paren nesting on one line of Zig, as (brace, paren).

    Brackets inside a `"..."` string, a `'.'` char literal, or a `//` comment
    are text. Counting them drifts the depth off the real nesting, and a
    drifted depth silently reassigns every line after it to the wrong class.
    `[` and `]` count with the parens: both continue a call across lines.
    """
    brace = paren = 0
    quote = None
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if quote:
            if c == "\\" and i + 1 < n:
                i += 2
                continue
            if c == quote:
                quote = None
            i += 1
            continue
        if c in '"\'':
            quote = c
        elif c == "/" and i + 1 < n and text[i + 1] == "/":
            break
        elif c == "{":
            brace += 1
        elif c == "}":
            brace -= 1
        elif c in "([":
            paren += 1
        elif c in ")]":
            paren -= 1
        i += 1
    return brace, paren


def statement_heads(lines: list[str]) -> list[int]:
    """Map each line index to the index of the line its statement starts on.

    A call spread over five lines is one decision, not five. Attributing the
    continuation lines to the head means `.err = @errorName(err),` counts as
    part of the log call above it instead of as four unrelated `other` lines,
    which is the difference between a class count that can be worked and one
    that is mostly punctuation.
    """
    heads: list[int] = []
    head = 0
    depth = 0
    for index, line in enumerate(lines):
        if depth <= 0:
            head = index
        heads.append(head)
        depth = max(0, depth + scan_deltas(line)[1])
    return heads


def errdefer_lines(lines: list[str]) -> set[int]:
    """Line indices inside an `errdefer { ... }` block, including its braces.

    The block form's body carries no `errdefer` keyword of its own, so without
    this every rung below the first would classify as ordinary code.
    """
    inside: set[int] = set()
    depth = 0
    open_at = -1
    for index, line in enumerate(lines):
        brace, _ = scan_deltas(line)
        if open_at < 0:
            if ERRDEFER_HEAD.match(line) and brace > 0:
                open_at = index
                depth = brace
                inside.add(index)
            continue
        inside.add(index)
        depth += brace
        if depth <= 0:
            open_at = -1
    return inside


def classify_line(text: str, in_errdefer: bool) -> str:
    """The class of one statement head.

    Order is precedence, not preference. A rung inside an `errdefer` block that
    also logs is cleanup first: it is reached by failing an allocation, which
    is the only fact that decides which test has to exist.
    """
    if in_errdefer or ERRDEFER_HEAD.match(text):
        return CLASS_ERRDEFER
    if ERROR_RETURN.search(text):
        return CLASS_ERROR_RETURN
    if FAILURE_RESPONSE.search(text):
        return CLASS_FAILURE_RESPONSE
    if FAILURE_LOG.search(text):
        return CLASS_FAILURE_LOG
    if BRACE_ONLY.match(text):
        return CLASS_BRACE
    return CLASS_OTHER


def read_unhit(report: Path) -> dict[str, set[int]]:
    """Every line the report records as never executed, keyed by source path."""
    if not report.exists():
        raise UsageError(
            f"no coverage report at {report} — run 'make test-unit-all' and "
            "'make test-integration' first, or pass --report"
        )
    unhit: dict[str, set[int]] = collections.defaultdict(set)
    for element in ET.parse(report).iter("class"):
        filename = element.get("filename")
        if not filename:
            continue
        for line in element.iter("line"):
            if line.get("hits") == "0":
                unhit[filename].add(int(line.get("number", "0")))
    return unhit


def classify_file(root: Path, path: str, numbers: set[int]) -> list[UnhitLine]:
    """Classify one file's unhit lines, dropping the ones no gate counts."""
    source = root / path
    if not source.exists():
        return []
    lines = source.read_text(errors="replace").splitlines()
    skip = inline_test_lines(str(source))
    heads = statement_heads(lines)
    blocks = errdefer_lines(lines)
    found: list[UnhitLine] = []
    for number in sorted(numbers):
        index = number - 1
        if index < 0 or index >= len(lines) or number in skip:
            continue
        head = heads[index]
        kind = classify_line(lines[head].strip(), head in blocks or index in blocks)
        found.append(UnhitLine(path, number, lines[index].strip(), kind))
    return found


def classify(report: Path, root: Path) -> list[UnhitLine]:
    """Every unhit line in the report, each carrying its class."""
    found: list[UnhitLine] = []
    for path, numbers in sorted(read_unhit(report).items()):
        found.extend(classify_file(root, path, numbers))
    return found


def parse_classes(raw: str | None) -> tuple[str, ...]:
    """The requested classes, defaulting to all of them, order preserved."""
    if not raw:
        return CLASS_NAMES
    wanted = [name.strip() for name in raw.split(",") if name.strip()]
    unknown = [name for name in wanted if name not in CLASS_NAMES]
    if unknown:
        raise UsageError(
            f"unknown class {', '.join(unknown)} — known: {', '.join(CLASS_NAMES)}"
        )
    return tuple(wanted)


def render(found: list[UnhitLine], wanted: tuple[str, ...], count_only: bool) -> str:
    """The report body: a bare count, or a line per finding under its class."""
    selected = [line for line in found if line.kind in wanted]
    if count_only:
        return str(len(selected))
    by_class = collections.defaultdict(list)
    for line in selected:
        by_class[line.kind].append(line)
    out: list[str] = []
    for name in wanted:
        rows = by_class.get(name, [])
        out.append(f"== {name} ({len(rows)}) ==")
        out.extend(f"  {row.path}:{row.number}  {row.text}" for row in rows)
    out.append(f"total {len(selected)}")
    return "\n".join(out)


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--repo-root", type=Path, default=Path("."))
    parser.add_argument("--class", dest="classes", default=None)
    parser.add_argument("--count", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        wanted = parse_classes(args.classes)
        found = classify(args.report, args.repo_root)
    except UsageError as err:
        print(f"✗ {err}", file=sys.stderr)
        return 2
    print(render(found, wanted, args.count))
    return 0


if __name__ == "__main__":
    sys.exit(main())
