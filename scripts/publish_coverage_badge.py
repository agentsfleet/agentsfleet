#!/usr/bin/env python3
"""Turn a graded coverage run into the badge payload the README renders.

The README badge must show what a run actually measured. The gate already
writes `zig_line_coverage_pct` to `.tmp/zig-coverage.txt` after grading, so the
figure exists; this is the only thing that carries it out.

Refuses to publish anything it cannot stand behind. A badge fed by a failed,
partial or absent run is worse than no badge — it reports a number over a suite
that did not finish, which is the exact misreading the coverage lane was rebuilt
to stop. Every refusal exits non-zero with the reason on stderr; the caller
decides whether that fails the build or merely skips publication.

Output is a shields.io endpoint document, so the README needs no build step and
the badge updates when the branch this writes to updates.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

SCHEMA_VERSION = 1
LABEL = "zig coverage"

# Keys the gate publishes. Both are required: a percentage without the
# denominator it was measured over is the number this lane exists to distrust.
KEY_PCT = "zig_line_coverage_pct"
KEY_LINES = "zig_measured_lines"
KEY_FILES = "zig_measured_files"
KEY_COMPONENTS_MEASURED = "zig_components_measured"
KEY_COMPONENTS_TOTAL = "zig_components_total"

# Colour bands. Chosen so the badge stops looking healthy well before the floor
# does — a number in the seventies should read as a problem at a glance.
COLOUR_BANDS = (
    (95.0, "brightgreen"),
    (90.0, "green"),
    (85.0, "yellowgreen"),
    (75.0, "yellow"),
    (60.0, "orange"),
)
COLOUR_FLOOR = "red"


class UngradedRun(RuntimeError):
    """The summary is absent, empty, or missing a key the badge needs."""


def parse_summary(text: str) -> dict[str, str]:
    """The gate's key/value surface as a mapping."""
    pairs = (line.partition("=") for line in text.splitlines() if line.strip())
    return {key.strip(): value.strip() for key, _separator, value in pairs}


def colour_for(percentage: float) -> str:
    """Badge colour for a measured rate."""
    for threshold, colour in COLOUR_BANDS:
        if percentage >= threshold:
            return colour
    return COLOUR_FLOOR


def read_measurement(summary_file: Path) -> tuple[float, int, int]:
    """Return (percentage, measured lines, measured files) from a graded run.

    Raises `UngradedRun` rather than defaulting, so a missing file can never
    become a published zero.
    """
    try:
        text = summary_file.read_text(encoding="utf-8")
    except FileNotFoundError as error:
        raise UngradedRun(f"no summary at {summary_file} — the gate did not run") from error

    keys = parse_summary(text)
    missing = [k for k in (KEY_PCT, KEY_LINES, KEY_FILES) if k not in keys]
    if missing:
        raise UngradedRun(f"summary is missing {', '.join(missing)} — the run did not grade")

    try:
        percentage = float(keys[KEY_PCT])
        lines = int(keys[KEY_LINES])
        files = int(keys[KEY_FILES])
    except ValueError as error:
        raise UngradedRun(f"summary holds a non-numeric measurement: {error}") from error

    if lines <= 0 or files <= 0:
        raise UngradedRun(
            f"the run measured {files} files / {lines} lines — a rate over nothing is not a rate"
        )
    if not 0.0 <= percentage <= 100.0:
        raise UngradedRun(f"measured rate {percentage} is outside 0-100")
    return percentage, lines, files


def full_capture(keys: dict[str, str]) -> bool:
    """True when every component the run declared actually collected.

    A rate over a subset flatters, so a partial capture must not publish. The
    keys are optional for older summaries; absence is treated as a full capture
    because those runs predate per-component accounting.
    """
    measured = keys.get(KEY_COMPONENTS_MEASURED)
    total = keys.get(KEY_COMPONENTS_TOTAL)
    if measured is None or total is None:
        return True
    return measured == total


def build_payload(percentage: float, lines: int, files: int) -> dict[str, object]:
    """The shields.io endpoint document."""
    return {
        "schemaVersion": SCHEMA_VERSION,
        "label": LABEL,
        "message": f"{percentage:.2f}%",
        "color": colour_for(percentage),
        # Not rendered by shields; carried so anyone opening the raw document
        # sees what the percentage was measured over.
        "measuredLines": lines,
        "measuredFiles": files,
    }


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--summary-file", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        percentage, lines, files = read_measurement(args.summary_file)
        keys = parse_summary(args.summary_file.read_text(encoding="utf-8"))
        if not full_capture(keys):
            raise UngradedRun(
                f"only {keys[KEY_COMPONENTS_MEASURED]} of "
                f"{keys[KEY_COMPONENTS_TOTAL]} components collected — a rate over a "
                "subset flatters and must not be published"
            )
    except UngradedRun as error:
        print(f"✗ refusing to publish a coverage badge: {error}", file=sys.stderr)
        return 1

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(build_payload(percentage, lines, files), indent=2) + "\n", encoding="utf-8"
    )
    print(f"✓ [badge] {percentage:.2f}% over {lines} lines in {files} files -> {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
