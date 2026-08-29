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
from collections.abc import Sequence
from pathlib import Path
from typing import TextIO

# `test result: ok. 12 passed; 0 failed; ...` — one per test binary, and a
# workspace run produces dozens. The count that matters is their sum.
PASSED = re.compile(r"^test result:.*?\b(\d+) passed\b", re.MULTILINE)


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


def run(command: Sequence[str], cwd: Path, tally_path: Path) -> tuple[int, int]:
    """Stream `command`, returning its real status and reported pass count."""
    tally = _open_tally(tally_path)
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
            return 127, 0

        if child.stdout is None:
            child.kill()
            child.wait()
            print("cannot read Rust lane command output", file=sys.stderr)
            return 1, 0

        with child.stdout:
            for line in child.stdout:
                sys.stdout.write(line)
                sys.stdout.flush()
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

        return child_status(child.wait()), passed
    finally:
        if tally is not None:
            _close_tally(tally, tally_path)


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

    status, ran = run(command, args.cwd, Path(args.tally))
    code, message = verdict_from_total(ran, status)
    print(f"{'✓' if code == 0 else '✗'} {args.label} {message}")
    return code


if __name__ == "__main__":
    sys.exit(main())
