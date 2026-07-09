#!/usr/bin/env python3
"""Permanent gate: every Zig `test` block on disk either runs, or is waived.

Zig registers a file's `test` blocks only when the file is force-referenced at
comptime from the root of a test compilation -- `test { _ = @import("x.zig"); }`
or `comptime { _ = x; }`. A plain `const x = @import("x.zig")` registers nothing,
even when the importing file is itself compiled. Nothing enforced that convention,
so a `test` block could sit on disk for months without ever compiling. It cannot
pass, fail, or skip: it is simply absent. `src/agentsfleetd/cmd/common.zig` held a
`migrations.len == 26` assertion against a 27-migration array while the suite
reported zero failures, because the block never compiled.

The authority here is the compiler, not a static import walk. A relative-import
walk under-predicts liveness (named modules, `comptime` references, barrels that
re-export) and would fail live files. Instead `zig build list-tests` compiles each
test binary a second time with `src/build/test_runner_list.zig` swapped in, which
prints the tests the compiler actually registered. The real `test` steps keep
Zig's default runner and are never touched.

Name format, per `builtin.test_functions`:

    <namespace>.test.<description>     named   `test "foo" {}`
    <namespace>.test_<N>              anonymous `test {}`

where <namespace> is the source path relative to that binary's root directory,
with `/` replaced by `.` and `.zig` stripped. We invert that mapping rather than
parse it: for each candidate file we compute its expected namespace and look for a
registered name carrying that prefix. Matching on the test *description* instead
would be unsound -- two files may share a description, and a live file would then
mask a dead twin.

Usage:
    check_zig_test_reachability.py --check
        exit 0 iff every src/**/*.zig containing a `test "` line registers at
        least one test in some binary, or carries `// no-test-root: <reason>`.
        Otherwise exit 1, listing each offending path.

    check_zig_test_reachability.py --count
        print `reachable_test_cases=<N>` and `reachable_integration_cases=<M>`,
        counting only blocks in files proven reachable. Consumed by
        `_lint_zig_test_depth` (make/quality.mk).
"""

import argparse
import collections
import os
import subprocess
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Source line that makes a file a candidate, and the integration sub-class the
# depth gate counts separately. Column-0 anchored, matching the historical gate.
TEST_LINE_PREFIX = 'test "'
INTEGRATION_LINE_PREFIX = 'test "integration:'

# Opt-out. A file whose tests only compile under a filtered or special build
# states why here, in a line of its own.
WAIVER_MARKER = "// no-test-root:"

# Wire format emitted by src/build/test_runner_list.zig.
ROOT_PREFIX = "ROOT\t"
TEST_PREFIX = "TEST\t"

# Registered-name infixes separating a file's namespace from its test.
NAMED_TEST_INFIX = ".test."
ANON_TEST_INFIX = ".test_"

LIST_STEP = "list-tests"
# The two build graphs. `build_runner.zig` is the runner daemon's own graph and
# defines its own `list-tests` step; neither knows about the other's binaries.
BUILD_GRAPHS = ((), ("--build-file", "build_runner.zig"))

SRC_DIR = "src"
ZIG_EXT = ".zig"


def registered_names():
    """Map each test binary's root directory to the set of names it registered.

    Two binaries may share a root directory (the runner's unit and integration
    lanes both root in src/runner/), so names accumulate per directory.
    """
    groups = collections.defaultdict(set)
    for graph in BUILD_GRAPHS:
        cmd = ["zig", "build", *graph, LIST_STEP]
        proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
        if proc.returncode != 0:
            sys.stderr.write(f"{' '.join(cmd)} failed:\n{proc.stderr}")
            sys.exit(1)
        root = None
        for line in proc.stdout.splitlines():
            if line.startswith(ROOT_PREFIX):
                root = line[len(ROOT_PREFIX):]
            elif line.startswith(TEST_PREFIX) and root is not None:
                groups[root].add(line[len(TEST_PREFIX):])
    if not groups:
        sys.stderr.write(f"no `{LIST_STEP}` output parsed -- is the lane wired?\n")
        sys.exit(1)
    return groups


def read_lines(path):
    with open(os.path.join(REPO_ROOT, path), errors="ignore") as handle:
        return handle.readlines()


def candidate_files():
    """Every tracked src/**/*.zig carrying at least one column-0 `test "` line."""
    proc = subprocess.run(
        ["git", "ls-files", SRC_DIR],
        cwd=REPO_ROOT, capture_output=True, text=True, check=True,
    )
    found = []
    for path in proc.stdout.split():
        if not path.endswith(ZIG_EXT):
            continue
        if any(line.startswith(TEST_LINE_PREFIX) for line in read_lines(path)):
            found.append(path)
    return sorted(found)


def is_live(path, groups):
    """True when `path` registers >=1 test in any binary rooted above it."""
    for root, names in groups.items():
        prefix = root + "/"
        if not path.startswith(prefix):
            continue
        namespace = path[len(prefix):-len(ZIG_EXT)].replace("/", ".")
        named, anon = namespace + NAMED_TEST_INFIX, namespace + ANON_TEST_INFIX
        if any(n.startswith(named) or n.startswith(anon) for n in names):
            return True
    return False


def is_waived(path):
    return any(WAIVER_MARKER in line for line in read_lines(path))


def count_blocks(path):
    """(named test blocks, integration blocks) declared in `path`."""
    lines = read_lines(path)
    total = sum(1 for line in lines if line.startswith(TEST_LINE_PREFIX))
    integration = sum(1 for line in lines if line.startswith(INTEGRATION_LINE_PREFIX))
    return total, integration


def run_check(groups, candidates):
    dead = [p for p in candidates if not is_live(p, groups) and not is_waived(p)]
    if dead:
        sys.stderr.write(
            f"✗ [zig] {len(dead)} test-bearing file(s) register no test in any binary.\n"
            f"  Force-import each from a test root, or add `{WAIVER_MARKER} <reason>`:\n"
        )
        for path in dead:
            blocks, _ = count_blocks(path)
            sys.stderr.write(f"    {path}  ({blocks} dead block(s))\n")
        return 1
    waived = [p for p in candidates if is_waived(p)]
    suffix = f" ({len(waived)} waived)" if waived else ""
    print(f"✓ [zig] test-root reachability: {len(candidates)} file(s) reachable{suffix}")
    return 0


def run_count(groups, candidates):
    unit = integration = 0
    for path in candidates:
        if not is_live(path, groups):
            continue
        blocks, integration_blocks = count_blocks(path)
        unit += blocks
        integration += integration_blocks
    print(f"reachable_test_cases={unit}")
    print(f"reachable_integration_cases={integration}")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="fail on any dead file")
    group.add_argument("--count", action="store_true", help="print reachable counts")
    args = parser.parse_args()

    groups = registered_names()
    candidates = candidate_files()
    if args.check:
        return run_check(groups, candidates)
    return run_count(groups, candidates)


if __name__ == "__main__":
    sys.exit(main())
