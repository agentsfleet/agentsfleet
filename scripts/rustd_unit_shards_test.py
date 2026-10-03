#!/usr/bin/env python3
"""Self-tests for the Rust unit lane's shards.

`test-unit-rustd` is three targets, one per crate family, and
`.github/workflows/test-unit-rustd.yml` runs them on three runners at once. The
split is only safe if it is total: every workspace member lands in exactly one
shard, the aggregate still runs all three, the workflow runs exactly those
shards as jobs of the same names, a shard that selects nothing fails rather than passes, and the
lane's verdict is green only when every job before it was.

The partition tests read `make -n`, so they compile nothing. The empty-shard
test builds one test-free crate in a temporary workspace, so it needs cargo,
which every caller of `make lint-all` already has for `lint-rustd`.

Run: python3 -m unittest discover -s scripts -t scripts -p 'rustd_unit_shards_test.py'
"""
from __future__ import annotations

import os
import re
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
RUSTD = REPO_ROOT / "rustd"
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "test-unit-rustd.yml"

AGGREGATE_TARGET = "test-unit-rustd"
SHARD_TARGET_PREFIX = "test-unit-rustd-"
VERDICT_JOB = "test-unit-rustd"
NEEDS_EXPRESSION = "${{ join(needs.*.result, ' ') }}"
BENCH_CRATE = "afd_bench"
DAEMON_LIBS = "daemon-libs"
# The shards, in the order `test-unit-rustd` runs them.
UNIT_SHARDS = ("runner", "daemon", DAEMON_LIBS)
NO_RULE = "No rule to make target"
NO_TESTS = "ran no tests"

# One `cargo test` invocation inside a dry-run recipe, up to the end of its
# argument list: `_rust_lane` closes it with ` ;` inside the single-quoted script.
CARGO_TEST = re.compile(r"cargo test --all-features(?P<args>[^;'}]*)")
JOB_HEADER = re.compile(r"^  (?P<name>[a-z][a-z0-9-]*):\s*$")
RUN_BLOCK = re.compile(r"^(?P<indent>\s+)run: \|\s*$")


def make_env() -> dict[str, str]:
    """The environment without a parent make's flags, which `make lint-all` exports."""
    return {key: value for key, value in os.environ.items() if key not in {"MAKEFLAGS", "MFLAGS", "MAKELEVEL"}}


def make(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["make", "--no-print-directory", *args],
        cwd=REPO_ROOT,
        env=make_env(),
        capture_output=True,
        text=True,
        check=False,
    )


def workspace_members() -> set[str]:
    with (RUSTD / "Cargo.toml").open("rb") as manifest:
        members = tomllib.load(manifest)["workspace"]["members"]
    names = set()
    for member in members:
        with (RUSTD / member / "Cargo.toml").open("rb") as manifest:
            names.add(tomllib.load(manifest)["package"]["name"])
    return names


def selection(args: str, members: set[str]) -> set[str]:
    """The packages one `cargo test` argument list selects."""
    tokens = args.split()
    flagged = {tokens[index + 1] for index, token in enumerate(tokens[:-1]) if token in {"-p", "--exclude"}}
    if "--workspace" in tokens:
        excluded = {tokens[index + 1] for index, token in enumerate(tokens[:-1]) if token == "--exclude"}
        return members - excluded
    return flagged


def invocations(target: str) -> list[str]:
    dry = make("-n", target)
    if dry.returncode != 0:
        raise AssertionError(f"make -n {target} failed:\n{dry.stdout}{dry.stderr}")
    return [match.group("args") for match in CARGO_TEST.finditer(dry.stdout)]


def shard_jobs() -> list[str]:
    """The workflow's shard jobs, in file order: every job named for a shard target."""
    names = (JOB_HEADER.match(line) for line in WORKFLOW.read_text(encoding="utf-8").splitlines())
    return [found.group("name") for found in names if found and found.group("name").startswith(SHARD_TARGET_PREFIX)]


def verdict_script() -> str:
    """The `run: |` block of the workflow's verdict job, dedented."""
    lines = WORKFLOW.read_text(encoding="utf-8").splitlines()
    in_job = False
    for index, line in enumerate(lines):
        header = JOB_HEADER.match(line)
        if header:
            in_job = header.group("name") == VERDICT_JOB
            continue
        block = RUN_BLOCK.match(line) if in_job else None
        if block:
            body_indent = len(block.group("indent")) + 2
            body = []
            for following in lines[index + 1:]:
                if following.strip() and len(following) - len(following.lstrip()) < body_indent:
                    break
                body.append(following[body_indent:])
            return "\n".join(body)
    raise AssertionError(f"no run block in the {VERDICT_JOB} job of {WORKFLOW}")


class Partition(unittest.TestCase):
    def test_unit_shards_partition_the_workspace(self):
        members = workspace_members()
        seen: dict[str, str] = {}
        for shard in UNIT_SHARDS:
            [args] = invocations(SHARD_TARGET_PREFIX + shard)
            for package in selection(args, members):
                self.assertNotIn(package, seen, f"{package} is in both {seen.get(package)} and {shard}")
                seen[package] = shard
        self.assertEqual(set(seen), members, "every workspace member lands in a shard")
        self.assertEqual(seen[BENCH_CRATE], DAEMON_LIBS, "afd_bench's tests stay in the unit lane")

    def test_unit_shards_default_to_every_shard(self):
        members = workspace_members()
        expected = [selection(invocations(SHARD_TARGET_PREFIX + shard)[0], members) for shard in UNIT_SHARDS]
        actual = [selection(args, members) for args in invocations(AGGREGATE_TARGET)]
        self.assertEqual(actual, expected, "the aggregate runs every shard, in partition order")


class Targets(unittest.TestCase):
    def test_every_shard_job_has_a_unit_target(self):
        targets = [SHARD_TARGET_PREFIX + shard for shard in UNIT_SHARDS]
        self.assertEqual(shard_jobs(), targets, "the workflow runs the shards test-unit-rustd runs")
        for target in shard_jobs():
            self.assertEqual(make("-n", target).returncode, 0, target)
        unknown = make("-n", SHARD_TARGET_PREFIX + "bogus")
        self.assertNotEqual(unknown.returncode, 0)
        self.assertIn(NO_RULE, unknown.stderr)
        self.assertNotIn("cargo test", unknown.stdout)

    def test_the_workflow_runs_the_shard_targets(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        for target in shard_jobs():
            self.assertIn(f"run: make {target}\n", text, "each shard job runs its own target")

    def test_empty_unit_shard_fails(self):
        with tempfile.TemporaryDirectory() as workspace:
            root = Path(workspace)
            crate = root / "empty"
            (crate / "src").mkdir(parents=True)
            (root / "Cargo.toml").write_text('[workspace]\nresolver = "3"\nmembers = ["empty"]\n', encoding="utf-8")
            (crate / "Cargo.toml").write_text(
                '[package]\nname = "empty"\nversion = "0.0.0"\nedition = "2024"\n', encoding="utf-8"
            )
            (crate / "src" / "lib.rs").write_text("//! No tests.\n", encoding="utf-8")
            run = make(f"{SHARD_TARGET_PREFIX}runner", f"RUSTD_DIR={root}", "_RUSTD_UNIT_RUNNER=empty")
        self.assertNotEqual(run.returncode, 0, run.stdout)
        self.assertIn(NO_TESTS, run.stdout)


class Verdict(unittest.TestCase):
    def verdict(self, *results: str) -> int:
        script = verdict_script().replace(NEEDS_EXPRESSION, " ".join(results))
        return subprocess.run(["bash", "-c", script], capture_output=True, text=True, check=False).returncode

    def test_verdict_fails_unless_every_needed_job_succeeded(self):
        self.assertEqual(self.verdict("success", "success", "success", "success"), 0)
        for other in ("failure", "skipped", "cancelled"):
            with self.subTest(result=other):
                self.assertNotEqual(self.verdict("success", "success", other, "success"), 0)

    def test_verdict_runs_when_a_dependency_failed(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        needs = ", ".join([*shard_jobs(), "lint-rustd"])
        self.assertIn(f"needs: [{needs}]", text)
        self.assertIn("if: ${{ !cancelled() }}", text)


if __name__ == "__main__":
    unittest.main()
