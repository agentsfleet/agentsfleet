#!/usr/bin/env python3
"""Run a Rust lane command and decide whether it actually passed.

Dimension 8.2. `make test-integration-rustd` and `make test-coverage-rustd` both
have to answer the same question — did the suite run, and did it pass — and both
had their own copy of the shell that answers it. Two copies of a guard is one
guard and one thing that looks like a guard.

Two ways a lane reports success without having earned it:

1. **Cargo failed and the pipe swallowed it.** This process owns Cargo directly,
   streams its combined output, and therefore retains the child's status without
   a shell pipeline or a writable status side channel.
2. **Nothing ran.** A `--ignored` selection that matches nothing exits 0 and
   prints `0 passed`, which reads exactly like a pass. So the tally is summed
   across every `test result:` line and zero is a failure.

Usage:
    rustd_lane_result.py --tally <log> --cwd <dir> --label <label> -- <command...>
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import time
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import TextIO

# `test result: ok. 12 passed; 0 failed; ...` — one per test binary, and a
# workspace run produces dozens. The count that matters is their sum.
PASSED = re.compile(r"^test result:.*?\b(\d+) passed\b", re.MULTILINE)

# Cargo prints exactly one `Finished ... profile ... in 3m 33s` when the last
# unit compiles, and every `Running <binary>` follows it. That line is the only
# boundary in the stream between paying for a build and paying for tests.
BUILD_DONE = re.compile(r"^\s*Finished\b")


class PhaseClock:
    """Splits one lane's wall time at the compile/execute boundary.

    The lane's total is not a comparable number on its own: a run that restored
    a warm `target` and a run that built cold differ by minutes for reasons that
    have nothing to do with the suite. Test-execution time is the half that
    compares across both, so the two are reported separately rather than summed
    into one figure that means different things on different runners.

    The clock is a parameter so a test can pin it; `time.monotonic` is immune to
    a wall-clock step, which a long lane can otherwise straddle.
    """

    def __init__(self, now: Callable[[], float] = time.monotonic) -> None:
        self._now = now
        self._started = now()
        self._built: float | None = None

    def mark(self, line: str) -> None:
        """Record the compile boundary the first time the stream shows it."""
        if self._built is None and BUILD_DONE.match(line):
            self._built = self._now()

    def phases(self) -> tuple[float | None, float | None, float]:
        """Compile, test, and total seconds; compile/test are None if unsplit.

        A stream with no `Finished` line — a build that failed before it got
        there — has no honest split, and reporting zero for it would invent one.
        """
        total = self._now() - self._started
        if self._built is None:
            return None, None, total
        return self._built - self._started, self._now() - self._built, total


def passed_total(output: str) -> int:
    """Total tests reported passing across every binary in `output`."""
    return sum(int(match) for match in PASSED.findall(output))


def verdict_from_total(ran: int, status: int) -> tuple[int, str]:
    """The lane's exit code and message from retained child evidence."""
    if status != 0:
        return status, "failed"
    if ran == 0:
        return 1, (
            "reported 0 passing tests — it did not run.\n"
            "  A selection that matches nothing exits 0 and reads as a pass; "
            "this is that check."
        )
    return 0, f"passed ({ran} tests)"


def verdict(output: str, status: int) -> tuple[int, str]:
    """Compatibility seam for callers classifying captured output."""
    return verdict_from_total(passed_total(output), status)


def child_status(returncode: int) -> int:
    """Translate a signal return into the shell status callers expect."""
    return returncode if returncode >= 0 else 128 + abs(returncode)


def _open_tally(path: Path) -> TextIO | None:
    """Open the optional diagnostic copy without making it a lane dependency."""
    try:
        return path.open("w", encoding="utf-8")
    except OSError as error:
        print(
            f"warning: cannot write Rust lane tally {path}: {error}",
            file=sys.stderr,
        )
        return None


def _close_tally(tally: TextIO, path: Path) -> None:
    """Close a diagnostic copy without replacing the command's result."""
    try:
        tally.close()
    except OSError as error:
        print(
            f"warning: could not finish Rust lane tally {path}: {error}",
            file=sys.stderr,
        )


def run(
    command: Sequence[str],
    cwd: Path,
    tally_path: Path,
    clock: PhaseClock | None = None,
) -> tuple[int, int, PhaseClock]:
    """Stream `command`, returning its real status, pass count, and phases."""
    tally = _open_tally(tally_path)
    clock = clock if clock is not None else PhaseClock()
    passed = 0
    try:
        try:
            child = subprocess.Popen(  # noqa: S603 - the repository supplies the command
                command,
                cwd=cwd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
            )
        except OSError as error:
            print(f"cannot start Rust lane command: {error}", file=sys.stderr)
            return 127, 0, clock

        if child.stdout is None:
            child.kill()
            child.wait()
            print("cannot read Rust lane command output", file=sys.stderr)
            return 1, 0, clock

        with child.stdout:
            for line in child.stdout:
                sys.stdout.write(line)
                sys.stdout.flush()
                clock.mark(line)
                passed += passed_total(line)
                if tally is not None:
                    try:
                        tally.write(line)
                    except OSError as error:
                        print(
                            f"warning: stopped writing Rust lane tally {tally_path}: {error}",
                            file=sys.stderr,
                        )
                        _close_tally(tally, tally_path)
                        tally = None

        return child_status(child.wait()), passed, clock
    finally:
        if tally is not None:
            _close_tally(tally, tally_path)


def phase_report(clock: PhaseClock, ran: int) -> str:
    """One machine-readable line of the evidence a speed claim is graded on.

    `key=value` rather than prose, because the reader is the next run's
    comparison, not a person; and `compile_s` stays separate from `tests_s`
    because a cached and an uncached runner agree on the second and never on
    the first.
    """
    compile_s, tests_s, total_s = clock.phases()
    fields = [
        f"compile_s={compile_s:.1f}" if compile_s is not None else "compile_s=unsplit",
        f"tests_s={tests_s:.1f}" if tests_s is not None else "tests_s=unsplit",
        f"total_s={total_s:.1f}",
        f"tests={ran}",
    ]
    return "  lane-phases " + " ".join(fields)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tally", required=True, help="path to the captured run output")
    parser.add_argument("--cwd", required=True, type=Path, help="command working directory")
    parser.add_argument("--label", required=True, help="what to call the lane in the message")
    parser.add_argument("command", nargs=argparse.REMAINDER, help="command to run after --")
    args = parser.parse_args()

    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("a command is required after --")

    status, ran, clock = run(command, args.cwd, Path(args.tally))
    code, message = verdict_from_total(ran, status)
    print(f"{'✓' if code == 0 else '✗'} {args.label} {message}")
    print(phase_report(clock, ran))
    return code


if __name__ == "__main__":
    sys.exit(main())
