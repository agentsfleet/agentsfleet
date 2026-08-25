#!/usr/bin/env python3
"""Decide whether a Rust lane run actually passed, from its own output.

Dimension 8.2. `make test-integration-rustd` and `make test-coverage-rustd` both
have to answer the same question — did the suite run, and did it pass — and both
had their own copy of the shell that answers it. Two copies of a guard is one
guard and one thing that looks like a guard.

Two ways a lane reports success without having earned it:

1. **Cargo failed and the pipe swallowed it.** The recipe pipes through `tee`,
   so `$?` is `tee`'s. The status is captured separately and handed here.
2. **Nothing ran.** A `--ignored` selection that matches nothing exits 0 and
   prints `0 passed`, which reads exactly like a pass. So the tally is summed
   across every `test result:` line and zero is a failure.

Usage:
    rustd_lane_result.py --tally <log> --status <n> --label "[rustd] Integration suite"
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# `test result: ok. 12 passed; 0 failed; ...` — one per test binary, and a
# workspace run produces dozens. The count that matters is their sum.
PASSED = re.compile(r"^test result:.*?\b(\d+) passed\b", re.MULTILINE)


def passed_total(output: str) -> int:
    """Total tests reported passing across every binary in `output`."""
    return sum(int(match) for match in PASSED.findall(output))


def verdict(output: str, status: int) -> tuple[int, str]:
    """The lane's exit code and the line to print with it."""
    if status != 0:
        return status, "failed"
    ran = passed_total(output)
    if ran == 0:
        return 1, (
            "reported 0 passing tests — it did not run.\n"
            "  A selection that matches nothing exits 0 and reads as a pass; "
            "this is that check."
        )
    return 0, f"passed ({ran} tests)"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tally", required=True, help="path to the captured run output")
    parser.add_argument("--status", required=True, type=int, help="cargo's own exit status")
    parser.add_argument("--label", required=True, help="what to call the lane in the message")
    args = parser.parse_args()

    output = Path(args.tally).read_text(encoding="utf-8", errors="replace")
    code, message = verdict(output, args.status)
    print(f"{'✓' if code == 0 else '✗'} {args.label} {message}")
    return code


if __name__ == "__main__":
    sys.exit(main())
