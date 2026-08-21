#!/usr/bin/env python3
"""Classify every `errdefer` rung by whether a missing one can leak in production.

The sweep's justification is operator-visible memory growth. That is not what a
missing rung costs everywhere: every HTTP handler runs under `hx.alloc`, a
per-request arena torn down when the request returns (`http/server.zig`), so a
rung the handler tree alone reaches frees with the request no matter what. Such
a rung is a latent defect — wrong the moment a non-arena caller appears — but it
cannot grow a daemon's resident size, and proving it consumes the same effort as
proving one that can.

This separates the two, so the sweep can be worked and graded on the rungs that
carry the justification:

  repeating   reached from a loop that runs for the process's life (cron fire
              service, queue workers, event bus, sweepers, runner daemon). A
              missing rung compounds per iteration.
  boot-once   reached only from startup or a one-shot command. Leaks once, dies
              with the process.
  arena       reached only from below the per-request arena. Cannot leak.
  unreached   no root reaches the file at all — triage input, not sweep work.

Method: reverse reachability over the `@import` graph. Traversal from every
long-lived root is cut at the handler tree, because that tree IS the arena
boundary — otherwise `cmd/serve.zig` reaching the server that dispatches to a
handler would mark the whole tree long-lived and the distinction would collapse.

Granularity is per FILE, and the direction of that error is deliberate: a file
is `arena` only when NO long-lived root reaches it, so the arena set never
absorbs a rung that can leak. The repeating set may contain a function only
handlers call, which makes it an over-estimate of the work and never an
under-estimate of the risk.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import defaultdict, deque
from pathlib import Path

CLASS_REPEATING = "repeating"
CLASS_BOOT_ONCE = "boot-once"
CLASS_ARENA = "arena"
CLASS_UNREACHED = "unreached"

#: Classes whose rungs can leak in a running daemon, and therefore carry the
#: milestone's justification. The sweep is graded on these.
LEAK_CAPABLE = (CLASS_REPEATING, CLASS_BOOT_ONCE)

IMPORT_RE = re.compile(r'@import\("([^"]+\.zig)"\)')
ERRDEFER_RE = re.compile(r"^\s*errdefer\b")

#: The handler tree is the arena boundary, not merely one more directory.
HANDLER_TREE = "agentsfleetd/http/handlers"

#: Entry points that keep running after a request returns. A rung reached from
#: one of these is allocated against an allocator that outlives every request.
REPEATING_ROOT_FILES = (
    "agentsfleetd/cron/main.zig",
    "agentsfleetd/cron/FireService.zig",
    "agentsfleetd/cron/FireQueue.zig",
    "agentsfleetd/queue/redis_subscriber.zig",
    "agentsfleetd/queue/fleet_ready.zig",
    "agentsfleetd/queue/connector_outbound.zig",
    "agentsfleetd/queue/redis_repair_verification.zig",
    "agentsfleetd/events/bus.zig",
    "agentsfleetd/events/subscription_hub.zig",
    "agentsfleetd/fleet/liveness_sweeper.zig",
    "agentsfleetd/fleet/reclaim_sweeper.zig",
    "agentsfleetd/fleet/repair_verification_dispatcher.zig",
    "agentsfleetd/fleet_runtime/approval_gate_sweeper.zig",
    "agentsfleetd/auth/clerk_fetch_worker.zig",
    "agentsfleetd/observability/otlp/exporter.zig",
    "agentsfleetd/cmd/serve_background.zig",
    "lib/call_deadline/scheduler.zig",
)
REPEATING_ROOT_DIRS = ("runner/daemon", "runner/engine")

#: Startup and one-shot commands. Their allocations die with the process.
BOOT_ROOT_DIRS = ("agentsfleetd/cmd", "agentsfleetd/config", "runner/cmd")


def is_test_path(path: str) -> bool:
    base = os.path.basename(path)
    return (
        path.endswith("_test.zig")
        or "/test/" in path
        or base.startswith("test_")
        or base.endswith("test_support.zig")
        or "test_harness" in path
        or "test_fixtures" in path
    )


def zig_sources(src_root: str) -> list[str]:
    found = []
    for dirpath, _dirs, names in os.walk(src_root):
        for name in names:
            if name.endswith(".zig"):
                found.append(os.path.normpath(os.path.join(dirpath, name)))
    return sorted(found)


def import_graph(files: list[str]) -> dict[str, set[str]]:
    edges: dict[str, set[str]] = defaultdict(set)
    existing = set(files)
    for path in files:
        try:
            text = Path(path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for match in IMPORT_RE.finditer(text):
            target = os.path.normpath(os.path.join(os.path.dirname(path), match.group(1)))
            if target in existing:
                edges[path].add(target)
    return edges


def reachable(edges, roots, blocked=frozenset()) -> set[str]:
    seen: set[str] = set()
    queue: deque[str] = deque()
    for root in roots:
        if root not in blocked:
            seen.add(root)
            queue.append(root)
    while queue:
        for nxt in edges[queue.popleft()]:
            if nxt not in blocked and nxt not in seen and not is_test_path(nxt):
                seen.add(nxt)
                queue.append(nxt)
    return seen


def count_rungs(path: str) -> int:
    try:
        text = Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return 0
    return sum(1 for line in text.splitlines() if ERRDEFER_RE.match(line))


def classify(src_root: str) -> list[dict]:
    files = [f for f in zig_sources(src_root) if not is_test_path(f)]
    edges = import_graph(files)

    handler_root = os.path.join(src_root, HANDLER_TREE)
    handlers = [f for f in files if f.startswith(handler_root + os.sep)]

    def under(prefixes) -> list[str]:
        return [
            f
            for f in files
            if any(f.startswith(os.path.join(src_root, p) + os.sep) for p in prefixes)
        ]

    repeating_roots = [
        os.path.join(src_root, p)
        for p in REPEATING_ROOT_FILES
        if os.path.exists(os.path.join(src_root, p))
    ] + under(REPEATING_ROOT_DIRS)

    # The handler tree is the arena boundary: never walk INTO it from a
    # long-lived root, or every handler inherits that root's lifetime.
    boundary = frozenset(handlers)
    arena_set = reachable(edges, handlers)
    repeating_set = reachable(edges, repeating_roots, boundary)
    boot_set = reachable(edges, under(BOOT_ROOT_DIRS), boundary)

    rows = []
    for path in files:
        rungs = count_rungs(path)
        if not rungs:
            continue
        if path in repeating_set:
            cls = CLASS_REPEATING
        elif path in boot_set:
            cls = CLASS_BOOT_ONCE
        elif path in arena_set:
            cls = CLASS_ARENA
        else:
            cls = CLASS_UNREACHED
        rows.append({"class": cls, "rungs": rungs, "file": os.path.relpath(path, src_root)})
    return rows


def shortest_path(edges, roots, target: str, blocked=frozenset()) -> list[str] | None:
    """One concrete import chain from a root to `target`, or None.

    The classifier answers "can this file leak"; a work list needs "through
    which caller", because a `repeating` file can hold functions only handlers
    reach and proving those is the cosmetic work this sweep excludes. Printing
    the chain turns a class label into something the author can check the
    function against.
    """
    prev: dict[str, str | None] = {}
    queue: deque[str] = deque()
    for root in roots:
        if root not in blocked:
            prev[root] = None
            queue.append(root)
    while queue:
        cur = queue.popleft()
        if cur == target:
            chain = []
            node: str | None = cur
            while node is not None:
                chain.append(node)
                node = prev[node]
            return list(reversed(chain))
        for nxt in edges[cur]:
            if nxt in blocked or nxt in prev or is_test_path(nxt):
                continue
            prev[nxt] = cur
            queue.append(nxt)
    return None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--src", default="src", help="source root to walk (default: src)")
    parser.add_argument(
        "--class",
        dest="only",
        help="comma-separated classes to report; 'leak-capable' expands to repeating,boot-once",
    )
    parser.add_argument("--count", action="store_true", help="print the rung total and nothing else")
    parser.add_argument("--files", action="store_true", help="list matching files, densest first")
    parser.add_argument("--json", action="store_true", help="emit every row as JSON")
    parser.add_argument(
        "--why",
        metavar="FILE",
        help="print one import chain from a long-lived root to FILE (path relative to --src), "
        "so a proof author can check whether the FUNCTION they are about to prove is on it",
    )
    args = parser.parse_args(argv)

    if args.why:
        files = [f for f in zig_sources(args.src) if not is_test_path(f)]
        edges = import_graph(files)
        target = os.path.normpath(os.path.join(args.src, args.why))
        if target not in files:
            print(f"no such source file: {args.why}", file=sys.stderr)
            return 2
        handler_root = os.path.join(args.src, HANDLER_TREE)
        handlers = [f for f in files if f.startswith(handler_root + os.sep)]

        def under(prefixes):
            return [
                f
                for f in files
                if any(f.startswith(os.path.join(args.src, p) + os.sep) for p in prefixes)
            ]

        roots = [
            os.path.join(args.src, p)
            for p in REPEATING_ROOT_FILES
            if os.path.exists(os.path.join(args.src, p))
        ] + under(REPEATING_ROOT_DIRS)
        chain = shortest_path(edges, roots, target, frozenset(handlers))
        label = CLASS_REPEATING
        if chain is None:
            chain = shortest_path(edges, under(BOOT_ROOT_DIRS), target, frozenset(handlers))
            label = CLASS_BOOT_ONCE
        if chain is None:
            print(f"{args.why}: no long-lived root reaches this file — every rung in it is arena-backed")
            return 0
        print(f"{args.why}: {label}, reached by")
        for depth, node in enumerate(chain):
            print(f"  {'  ' * depth}{os.path.relpath(node, args.src)}")
        print(
            "\nThe FILE is reachable. Confirm the FUNCTION you are proving is called on "
            "this chain — if every caller of it passes hx.alloc, its rungs are arena-backed."
        )
        return 0

    rows = classify(args.src)
    if args.only:
        wanted: set[str] = set()
        for token in args.only.split(","):
            token = token.strip()
            if token == "leak-capable":
                wanted.update(LEAK_CAPABLE)
            elif token:
                wanted.add(token)
        rows = [r for r in rows if r["class"] in wanted]

    if args.json:
        json.dump(rows, sys.stdout, indent=1)
        sys.stdout.write("\n")
        return 0
    if args.count:
        print(sum(r["rungs"] for r in rows))
        return 0
    if args.files:
        for row in sorted(rows, key=lambda r: (-r["rungs"], r["file"])):
            print(f"{row['rungs']:>4}  {row['class']:<10}  {row['file']}")
        return 0

    totals: dict[str, int] = defaultdict(int)
    counts: dict[str, int] = defaultdict(int)
    for row in rows:
        totals[row["class"]] += row["rungs"]
        counts[row["class"]] += 1
    print(f"{'class':<12}{'rungs':>7}{'files':>7}")
    for cls in (CLASS_REPEATING, CLASS_BOOT_ONCE, CLASS_ARENA, CLASS_UNREACHED):
        print(f"{cls:<12}{totals[cls]:>7}{counts[cls]:>7}")
    print(f"{'TOTAL':<12}{sum(totals.values()):>7}{sum(counts.values()):>7}")
    leak = sum(totals[c] for c in LEAK_CAPABLE)
    print(f"\nleak-capable (repeating + boot-once): {leak}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
