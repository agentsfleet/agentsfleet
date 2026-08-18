#!/usr/bin/env python3
"""Run one verification owner with a portable process-group timeout."""
import argparse
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def write_timing(path, label, started_at_ms, finished_at_ms, duration_ms,
                 outcome, exit_code):
    destination = Path(path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=destination.parent,
        prefix=destination.name + ".", delete=False,
    ) as stream:
        json.dump({
            "label": label,
            "started_at_ms": started_at_ms,
            "finished_at_ms": finished_at_ms,
            "duration_ms": duration_ms,
            "outcome": outcome,
            "exit_code": exit_code,
        }, stream, indent=2, sort_keys=True)
        stream.write("\n")
        temporary = stream.name
    os.replace(temporary, destination)


def run(command, seconds, label, timing_output=None, started_at_ms=None):
    started_at_ms = started_at_ms or time.time_ns() // 1_000_000
    exit_code = None
    outcome = "crashed"
    try:
        process = subprocess.Popen(command, start_new_session=True)
        try:
            exit_code = process.wait(timeout=seconds)
            outcome = "success" if exit_code == 0 else "failed"
        except subprocess.TimeoutExpired:
            print(f"✗ {label} timed out after {seconds}s", file=sys.stderr)
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            exit_code = 124
            outcome = "timeout"
        return exit_code
    finally:
        if timing_output:
            finished_at_ms = time.time_ns() // 1_000_000
            duration_ms = finished_at_ms - started_at_ms
            write_timing(timing_output, label, started_at_ms, finished_at_ms,
                         duration_ms, outcome, exit_code)


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--seconds", type=int, required=True)
    parser.add_argument("--label", required=True)
    parser.add_argument("--timing-output")
    parser.add_argument("--started-at-file")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if args.seconds <= 0 or not command:
        parser.error("a positive timeout and command are required")
    started_at_ms = None
    if args.started_at_file:
        started_at_ms = int(Path(args.started_at_file).read_text(encoding="utf-8"))
    return run(command, args.seconds, args.label, args.timing_output, started_at_ms)


if __name__ == "__main__":
    raise SystemExit(main())
