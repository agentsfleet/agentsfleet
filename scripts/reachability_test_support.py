#!/usr/bin/env python3
"""Shared fixtures for the check_zig_test_reachability self-tests.

Split out of the suite under RULE FLL. Nothing here shells out to `zig build`:
each case builds a fixture tree and a synthetic registered-name set, so the tests
stay fast and hermetic while the real listing is exercised by `make lint-zig`.
"""

import contextlib
import io
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_zig_test_reachability as checker  # noqa: E402

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# `lint-zig` lives in quality.mk; the two reachability recipes were split into
# reachability.mk under RULE FLL. The wiring tests read whichever holds the fact.
QUALITY_MK = os.path.join(REPO_ROOT, "make", "quality.mk")
REACHABILITY_MK = os.path.join(REPO_ROOT, "make", "reachability.mk")
CHECKER_TARGET = "_lint_zig_test_reachability"

ROOT_DIR = "src/agentsfleetd"


class ReachabilityTestCase(unittest.TestCase):
    """Points the checker at a throwaway tree so fixtures never touch the repo."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self._prev_root = checker.REPO_ROOT
        checker.REPO_ROOT = self._tmp.name
        self.addCleanup(self._restore)

    def _restore(self):
        checker.REPO_ROOT = self._prev_root
        self._tmp.cleanup()

    @property
    def tmp_dir(self):
        return self._tmp.name

    def write(self, rel_path, body):
        full = os.path.join(self._tmp.name, rel_path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as handle:
            handle.write(body)
        return rel_path

    def run_check(self, groups, candidates):
        err, out = io.StringIO(), io.StringIO()
        with contextlib.redirect_stderr(err), contextlib.redirect_stdout(out):
            code = checker.run_check(groups, candidates)
        return code, err.getvalue() + out.getvalue()

    def run_count(self, groups, candidates):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            checker.run_count(groups, candidates)
        return out.getvalue()


class FakeProc:
    def __init__(self, stdout="", returncode=0, stderr=""):
        self.stdout = stdout
        self.returncode = returncode
        self.stderr = stderr


class SubprocessFakeCase(ReachabilityTestCase):
    """Replaces subprocess.run with a scripted queue of results."""

    def fake_subprocess(self, *results):
        queue = list(results)
        original = checker.subprocess.run
        checker.subprocess.run = lambda *a, **k: queue.pop(0)
        self.addCleanup(lambda: setattr(checker.subprocess, "run", original))
