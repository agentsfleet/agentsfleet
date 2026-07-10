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

# Source lines that make a file a candidate. Column-0 anchored. Anonymous blocks
# (`test { ... }`) count for CANDIDACY but not for the depth total: a file holding
# only anonymous tests can still be unreachable, and skipping it would leave the
# gate with the blind spot it exists to close. The depth total stays on named
# blocks so it remains comparable to the historical `^test "` count.
CANDIDATE_LINE_PREFIXES = ('test "', "test {")
TEST_LINE_PREFIX = 'test "'
INTEGRATION_LINE_PREFIX = 'test "integration:'

# Opt-out. A file whose tests only compile under a filtered or special build
# states why here, in a line of its own.
WAIVER_MARKER = "// no-test-root:"

# Wire format emitted by src/build/test_runner_list.zig. Each TEST line is
# `TEST\t<root_dir>\t<name>` — self-describing, so attribution never depends on
# stdout ordering between concurrently-run lanes.
ROOT_PREFIX = "ROOT\t"
TEST_PREFIX = "TEST\t"
FIELD_SEP = "\t"

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
        for line in proc.stdout.splitlines():
            if line.startswith(ROOT_PREFIX):
                # Proves the lane ran. A lane that registers nothing still lands here,
                # so its files are correctly judged dead rather than silently skipped.
                groups.setdefault(line[len(ROOT_PREFIX):], set())
            elif line.startswith(TEST_PREFIX):
                root, _, name = line[len(TEST_PREFIX):].partition(FIELD_SEP)
                if name:
                    groups[root].add(name)
    if not groups:
        sys.stderr.write(f"no `{LIST_STEP}` output parsed -- is the lane wired?\n")
        sys.exit(1)
    return groups


def read_lines(path):
    with open(os.path.join(REPO_ROOT, path), errors="ignore") as handle:
        return handle.readlines()


def declares_a_test(path):
    return any(
        line.startswith(CANDIDATE_LINE_PREFIXES) for line in read_lines(path)
    )


def candidate_files():
    """Every src/**/*.zig declaring at least one column-0 `test` block.

    Walks the tree instead of shelling out to `git ls-files`: the Continuous
    Integration (CI) jobs run in a container where git refuses the checkout
    ("dubious ownership", exit 128), and the textual gate this replaces used
    `find src`, which has exactly these semantics.
    """
    found = []
    for dirpath, _dirs, filenames in os.walk(os.path.join(REPO_ROOT, SRC_DIR)):
        for name in filenames:
            if not name.endswith(ZIG_EXT):
                continue
            path = os.path.relpath(os.path.join(dirpath, name), REPO_ROOT)
            if declares_a_test(path):
                found.append(path)
    return sorted(found)  # os.walk order is filesystem-dependent


def has_ambiguous_name(path):
    """True when `path`'s namespace could collide with another file's.

    The namespace is the relative path with `/` rewritten to `.`, so `a/b/c.zig` and
    `a/b.c.zig` both yield `a.b.c` and a live file would mask a dead twin. Nothing in
    `src/` is named this way today; this keeps it that way rather than trusting it.
    """
    return "." in os.path.basename(path)[: -len(ZIG_EXT)]


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


def waiver_reason(path):
    """The reason after `// no-test-root:`, or None when the file is not waived.

    An empty reason does not waive. A silent opt-out is how a test block goes dark
    in the first place, so the marker must say why or it does not count.
    """
    for line in read_lines(path):
        marker = line.find(WAIVER_MARKER)
        if marker != -1:
            reason = line[marker + len(WAIVER_MARKER):].strip()
            return reason or None
    return None


def is_waived(path):
    return waiver_reason(path) is not None


def count_blocks(path):
    """(named test blocks, integration blocks) declared in `path`."""
    lines = read_lines(path)
    total = sum(1 for line in lines if line.startswith(TEST_LINE_PREFIX))
    integration = sum(1 for line in lines if line.startswith(INTEGRATION_LINE_PREFIX))
    return total, integration


def run_check(groups, candidates):
    """Dead files fail the gate. Waived files are named, never merely counted:
    a waiver that nobody reads is how a test block goes dark in the first place."""
    ambiguous = [p for p in candidates if has_ambiguous_name(p)]
    if ambiguous:
        sys.stderr.write(
            "✗ [zig] filename would produce an ambiguous test namespace "
            "(a dot before `.zig` collides with a directory separator):\n"
        )
        for path in ambiguous:
            sys.stderr.write(f"    {path}\n")
        return 1

    dead, waived, stale_waivers = [], [], []
    for path in candidates:
        reason = waiver_reason(path)
        if is_live(path, groups):
            if reason is not None:
                stale_waivers.append(path)
        elif reason is not None:
            waived.append((path, reason))
        else:
            dead.append(path)

    if dead:
        sys.stderr.write(
            f"✗ [zig] {len(dead)} test-bearing file(s) register no test in any binary.\n"
            f"  Force-import each from a test root, or add `{WAIVER_MARKER} <reason>`:\n"
        )
        for path in dead:
            # Declared, not counted: an anonymous-only file has zero *named* blocks
            # yet is still dead, and "(0 dead blocks)" would read like a false alarm.
            declared = sum(
                1 for line in read_lines(path)
                if line.startswith(CANDIDATE_LINE_PREFIXES)
            )
            sys.stderr.write(f"    {path}  ({declared} dead block(s))\n")
        return 1

    for path, reason in waived:
        print(f"  waived: {path} — {reason or '<no reason given>'}")
    for path in stale_waivers:
        print(f"  stale waiver: {path} registers tests; drop its `{WAIVER_MARKER}` line")
    reachable = len(candidates) - len(waived)
    suffix = f", {len(waived)} waived" if waived else ""
    print(f"✓ [zig] test-root reachability: {reachable} file(s) reachable{suffix}")
    return 0


def format_counts(groups, candidates):
    """Blocks declared in files the compiler proved live.

    Not the registered-name count: a file reachable from two roots (every
    `src/agentsfleetd/auth/**` file registers in both the daemon and auth binaries)
    would be counted twice, and anonymous barrel `test {}` blocks would be credited.
    Registration is per-file, so per-file counting is both compiler-grounded and
    directly comparable to the historical textual total.
    """
    unit = integration = 0
    for path in candidates:
        if not is_live(path, groups):
            continue
        blocks, integration_blocks = count_blocks(path)
        unit += blocks
        integration += integration_blocks
    return f"reachable_test_cases={unit}\nreachable_integration_cases={integration}\n"


def run_count(groups, candidates):
    sys.stdout.write(format_counts(groups, candidates))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--check", action="store_true", help="fail on any dead file")
    group.add_argument("--count", action="store_true", help="print reachable counts")
    # Listing all 8 binaries costs ~10s, so `--check` can hand its counts to the depth
    # gate instead of making it pay for a second, identical listing.
    parser.add_argument("--counts-out", metavar="PATH", help="also write counts to PATH")
    args = parser.parse_args()

    groups = registered_names()
    candidates = candidate_files()
    if args.count:
        return run_count(groups, candidates)

    code = run_check(groups, candidates)
    if code == 0 and args.counts_out:
        with open(args.counts_out, "w") as handle:
            handle.write(format_counts(groups, candidates))
    return code


if __name__ == "__main__":
    sys.exit(main())
