#!/usr/bin/env python3
"""Self-tests for `make test-unit-runner`'s Zig version guard.

The runner builds and ships on the one Zig `build.zig.zon` names. A newer Zig
accepts that manifest's floor and then fails on standard-library drift far from
the cause, so the target refuses any other version before it builds anything.
These drive the real target with a stand-in `zig` first on `PATH`: one that
reports another version must be refused by name, and one that reports the
pinned version must be handed exactly the runner's test build.

Run: python3 -m unittest discover -s scripts -t scripts -p '*_test.py'
"""
from __future__ import annotations

import os
import re
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
MANIFEST = REPO_ROOT / "build.zig.zon"
TARGET = "test-unit-runner"
PINNED = re.compile(r'^\s*\.minimum_zig_version = "([^"]+)",', re.MULTILINE)
WRONG_VERSION = "0.15.2"
RUNNER_BUILD = "build --build-file build_runner.zig test"
# The stand-in answers `zig version` with $FAKE_ZIG_VERSION and records any
# other invocation's arguments, one line each, in $FAKE_ZIG_LOG.
STAND_IN = """#!/bin/sh
if [ "$1" = "version" ]; then echo "$FAKE_ZIG_VERSION"; exit 0; fi
echo "$*" >> "$FAKE_ZIG_LOG"
"""


def pinned_version() -> str:
    match = PINNED.search(MANIFEST.read_text(encoding="utf-8"))
    if match is None:
        raise AssertionError(f"{MANIFEST} names no minimum_zig_version")
    return match.group(1)


class RunnerZigVersionGuardTest(unittest.TestCase):
    def run_target(self, version: str) -> tuple[subprocess.CompletedProcess[str], list[str]]:
        with tempfile.TemporaryDirectory() as scratch:
            bin_dir = Path(scratch)
            zig = bin_dir / "zig"
            zig.write_text(STAND_IN, encoding="utf-8")
            zig.chmod(zig.stat().st_mode | stat.S_IXUSR)
            log = bin_dir / "calls.log"
            env = {
                **os.environ,
                "PATH": f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}",
                "FAKE_ZIG_VERSION": version,
                "FAKE_ZIG_LOG": str(log),
                # The progress wrapper's heartbeat is noise in a captured run.
                "WITH_PROGRESS_DISABLE": "1",
            }
            result = subprocess.run(
                ["make", "-s", TARGET],
                cwd=REPO_ROOT,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            calls = log.read_text(encoding="utf-8").splitlines() if log.exists() else []
        return result, calls

    def test_runner_zig_version_guard(self) -> None:
        result, calls = self.run_target(WRONG_VERSION)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(
            f"zig {WRONG_VERSION} found; the runner's tests need Zig {pinned_version()} (build.zig.zon)",
            result.stdout + result.stderr,
        )
        self.assertEqual(calls, [], "a refused version must build nothing")

    def test_runner_zig_version_match_runs_the_runner_build(self) -> None:
        result, calls = self.run_target(pinned_version())
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(calls, [RUNNER_BUILD])


if __name__ == "__main__":
    unittest.main()
