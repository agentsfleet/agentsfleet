#!/usr/bin/env python3
"""Record and compare equivalent Rust coverage-lane benchmark evidence."""
from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TextIO

from rustd_lane_result import passed_total

SCHEMA_VERSION = 1
MAX_WALL_RATIO = 0.80


class EvidenceError(ValueError):
    """Benchmark evidence is incomplete or not comparable."""


@dataclass(frozen=True)
class Run:
    """One complete coverage-lane observation."""

    wall_seconds: float
    artifact_bytes: int
    test_count: int
    covered_lines: int
    total_lines: int


@dataclass(frozen=True)
class Evidence:
    """Equivalent-run metadata and its observations."""

    toolchain: str
    runner: str
    reset_contract: str
    runs: tuple[Run, ...]

    @classmethod
    def read(cls, path: Path) -> Evidence:
        """Read validated benchmark evidence from `path`.

        Raises:
            EvidenceError: when required evidence is absent or malformed.
        """
        try:
            raw: Any = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise EvidenceError(f"cannot read {path}: {error}") from error
        if not isinstance(raw, dict) or raw.get("schema") != SCHEMA_VERSION:
            raise EvidenceError(f"{path}: unsupported or absent schema")
        try:
            runs = tuple(
                Run(
                    wall_seconds=float(item["wall_seconds"]),
                    artifact_bytes=int(item["artifact_bytes"]),
                    test_count=int(item["test_count"]),
                    covered_lines=int(item["covered_lines"]),
                    total_lines=int(item["total_lines"]),
                )
                for item in raw["runs"]
            )
            evidence = cls(
                toolchain=str(raw["toolchain"]),
                runner=str(raw["runner"]),
                reset_contract=str(raw["reset_contract"]),
                runs=runs,
            )
        except (KeyError, TypeError, ValueError) as error:
            raise EvidenceError(f"{path}: malformed evidence: {error}") from error
        evidence.validate(path)
        return evidence

    def write(self, path: Path) -> None:
        """Write validated evidence in the stable comparison schema."""
        self.validate(path)
        payload = {
            "schema": SCHEMA_VERSION,
            "toolchain": self.toolchain,
            "runner": self.runner,
            "reset_contract": self.reset_contract,
            "runs": [run.__dict__ for run in self.runs],
        }
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    def validate(self, path: Path) -> None:
        """Reject evidence that cannot support a performance claim."""
        if not self.toolchain or not self.runner or not self.reset_contract:
            raise EvidenceError(f"{path}: comparison metadata must be non-empty")
        if len(self.runs) < 3:
            raise EvidenceError(f"{path}: at least three complete runs are required")
        for run in self.runs:
            if (
                run.wall_seconds <= 0
                or run.artifact_bytes <= 0
                or run.test_count <= 0
                or run.covered_lines <= 0
                or run.total_lines <= 0
                or run.covered_lines > run.total_lines
            ):
                raise EvidenceError(f"{path}: every run must contain positive, coherent evidence")


def artifact_bytes(root: Path) -> int:
    """Return allocated bytes for files under an artifact root."""
    try:
        return sum(
            entry.stat(follow_symlinks=False).st_blocks * 512
            for directory, _children, files in os.walk(root)
            for name in files
            if (entry := Path(directory, name)).exists()
        )
    except OSError as error:
        raise EvidenceError(f"cannot measure artifacts under {root}: {error}") from error


def coverage_lines(path: Path) -> tuple[int, int]:
    """Count unique covered and product lines in an LCOV report."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise EvidenceError(f"cannot read coverage report {path}: {error}") from error

    source: str | None = None
    counts: dict[tuple[str, int], int] = {}
    for line in lines:
        if line.startswith("SF:"):
            source = line[3:]
        elif source is not None and line.startswith("DA:"):
            try:
                line_number, count, *_rest = line[3:].split(",")
                key = (source, int(line_number))
                counts[key] = max(counts.get(key, 0), int(count))
            except ValueError as error:
                raise EvidenceError(f"{path}: malformed LCOV line {line!r}") from error
    if not counts:
        raise EvidenceError(f"{path}: no instrumented lines found")
    return sum(count > 0 for count in counts.values()), len(counts)


def run_once(
    command: list[str], cwd: Path, artifacts: Path, coverage: Path, output: TextIO
) -> Run:
    """Run one canonical lane while collecting its observable evidence."""
    started = time.monotonic()
    try:
        child = subprocess.Popen(  # noqa: S603 - the caller supplies the canonical command
            command,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
    except OSError as error:
        raise EvidenceError(f"cannot start benchmark command: {error}") from error
    if child.stdout is None:
        child.kill()
        child.wait()
        raise EvidenceError("cannot read benchmark command output")

    tests = 0
    with child.stdout:
        for line in child.stdout:
            output.write(line)
            output.flush()
            tests += passed_total(line)
    status = child.wait()
    if status != 0:
        raise EvidenceError(f"benchmark command failed with status {status}")
    covered, total = coverage_lines(coverage)
    return Run(
        wall_seconds=time.monotonic() - started,
        artifact_bytes=artifact_bytes(artifacts),
        test_count=tests,
        covered_lines=covered,
        total_lines=total,
    )


def compare(before: Evidence, after: Evidence) -> tuple[bool, str]:
    """Compare equivalent run sets and return a gate verdict plus summary."""
    before_shape = (before.toolchain, before.runner, before.reset_contract)
    after_shape = (after.toolchain, after.runner, after.reset_contract)
    if before_shape != after_shape:
        raise EvidenceError("toolchain, runner, and datastore reset contract must match")

    before_total = {run.total_lines for run in before.runs}
    after_total = {run.total_lines for run in after.runs}
    if len(before_total) != 1 or before_total != after_total:
        raise EvidenceError("coverage denominator must be stable across every run")

    before_tests = min(run.test_count for run in before.runs)
    after_tests = min(run.test_count for run in after.runs)
    if after_tests < before_tests:
        raise EvidenceError("after evidence discovered fewer tests than the baseline")

    before_wall = statistics.median(run.wall_seconds for run in before.runs)
    after_wall = statistics.median(run.wall_seconds for run in after.runs)
    ratio = after_wall / before_wall
    before_bytes = int(statistics.median(run.artifact_bytes for run in before.runs))
    after_bytes = int(statistics.median(run.artifact_bytes for run in after.runs))
    covered = min(run.covered_lines for run in after.runs)
    total = after_total.pop()
    passed = ratio <= MAX_WALL_RATIO
    summary = (
        f"wall median {before_wall:.2f}s -> {after_wall:.2f}s "
        f"(ratio {ratio:.3f}, required <= {MAX_WALL_RATIO:.2f}); "
        f"artifact median {before_bytes} -> {after_bytes} bytes; "
        f"tests >= {after_tests}; coverage >= {covered}/{total}"
    )
    return passed, summary


def main() -> int:
    """CLI entry point."""
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    compare_parser = subcommands.add_parser("compare", help="compare before and after JSON")
    compare_parser.add_argument("before", type=Path)
    compare_parser.add_argument("after", type=Path)
    record_parser = subcommands.add_parser("record", help="record equivalent canonical runs")
    record_parser.add_argument("output", type=Path)
    record_parser.add_argument("--toolchain", required=True)
    record_parser.add_argument("--runner", required=True)
    record_parser.add_argument("--reset-contract", required=True)
    record_parser.add_argument("--cwd", required=True, type=Path)
    record_parser.add_argument("--artifacts", required=True, type=Path)
    record_parser.add_argument("--coverage", required=True, type=Path)
    record_parser.add_argument("--runs", type=int, default=3)
    record_parser.add_argument("lane_command", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    try:
        if args.command == "record":
            if args.runs < 3:
                raise EvidenceError("at least three complete runs are required")
            command = args.lane_command[1:] if args.lane_command[:1] == ["--"] else args.lane_command
            if not command:
                raise EvidenceError("a canonical lane command is required after --")
            runs = tuple(
                run_once(command, args.cwd, args.artifacts, args.coverage, sys.stdout)
                for _run in range(args.runs)
            )
            evidence = Evidence(args.toolchain, args.runner, args.reset_contract, runs)
            evidence.write(args.output)
            print(f"recorded {len(runs)} equivalent runs in {args.output}")
            return 0

        passed, summary = compare(Evidence.read(args.before), Evidence.read(args.after))
    except EvidenceError as error:
        print(f"benchmark evidence rejected: {error}", file=sys.stderr)
        return 2
    print(summary)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
