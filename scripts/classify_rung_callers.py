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

What counts as an edge lives in `rung_call_edges.py`: an import taken for a type
or a constant is not one. `--pruned` lists what that dropped.

Granularity is per FILE, and the direction of that error is deliberate: a file
is `arena` only when NO long-lived root reaches it, so the arena set never
absorbs a rung that can leak. The repeating set may still contain a function only
handlers call, which makes it an over-estimate of the work and never an
under-estimate of the risk. `--fn FILE:NAME` closes that last gap for one
function: it classifies every caller that actually calls NAME, so a proof author
learns whether ANY of them outlives a request before writing a line.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from collections import defaultdict, deque

from rung_call_edges import callers_of, import_graph, is_test_path, read_source, zig_sources

CLASS_REPEATING = "repeating"
CLASS_BOOT_ONCE = "boot-once"
CLASS_ARENA = "arena"
CLASS_UNREACHED = "unreached"

#: Classes whose rungs can leak in a running daemon, and therefore carry the
#: milestone's justification. The sweep is graded on these.
LEAK_CAPABLE = (CLASS_REPEATING, CLASS_BOOT_ONCE)

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
    return sum(1 for line in read_source(path).splitlines() if ERRDEFER_RE.match(line))


def long_lived_roots(src_root: str, files: list[str]) -> tuple[list[str], list[str]]:
    """(repeating roots, boot-once roots) that exist in this tree."""

    def under(prefixes) -> list[str]:
        return [
            f
            for f in files
            if any(f.startswith(os.path.join(src_root, p) + os.sep) for p in prefixes)
        ]

    repeating = [
        os.path.join(src_root, p)
        for p in REPEATING_ROOT_FILES
        if os.path.exists(os.path.join(src_root, p))
    ] + under(REPEATING_ROOT_DIRS)
    return repeating, under(BOOT_ROOT_DIRS)


def class_map(src_root: str, files: list[str], edges) -> dict[str, str]:
    handler_root = os.path.join(src_root, HANDLER_TREE)
    handlers = [f for f in files if f.startswith(handler_root + os.sep)]
    repeating_roots, boot_roots = long_lived_roots(src_root, files)

    # The handler tree is the arena boundary: never walk INTO it from a
    # long-lived root, or every handler inherits that root's lifetime.
    boundary = frozenset(handlers)
    arena_set = reachable(edges, handlers)
    repeating_set = reachable(edges, repeating_roots, boundary)
    boot_set = reachable(edges, boot_roots, boundary)

    classes = {}
    for path in files:
        if path in repeating_set:
            classes[path] = CLASS_REPEATING
        elif path in boot_set:
            classes[path] = CLASS_BOOT_ONCE
        elif path in arena_set:
            classes[path] = CLASS_ARENA
        else:
            classes[path] = CLASS_UNREACHED
    return classes


def classify(src_root: str) -> list[dict]:
    files = [f for f in zig_sources(src_root) if not is_test_path(f)]
    classes = class_map(src_root, files, import_graph(files))
    rows = []
    for path in files:
        rungs = count_rungs(path)
        if not rungs:
            continue
        rows.append(
            {"class": classes[path], "rungs": rungs, "file": os.path.relpath(path, src_root)}
        )
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
    parser.add_argument(
        "--fn",
        metavar="FILE:NAME",
        help="classify every caller of one function, so a proof author learns whether ANY "
        "of them outlives a request before writing a line",
    )
    parser.add_argument(
        "--pruned",
        action="store_true",
        help="list the import edges dropped as type-only or constant-only",
    )
    args = parser.parse_args(argv)

    if args.why or args.fn or args.pruned:
        pruned: list[tuple[str, str]] = []
        files = [f for f in zig_sources(args.src) if not is_test_path(f)]
        edges = import_graph(files, pruned)
        handler_root = os.path.join(args.src, HANDLER_TREE)
        handlers = [f for f in files if f.startswith(handler_root + os.sep)]
        repeating_roots, boot_roots = long_lived_roots(args.src, files)

    if args.pruned:
        print(f"{len(pruned)} import edges dropped as type-only or constant-only:")
        for importer, target in sorted(pruned):
            print(
                f"  {os.path.relpath(importer, args.src)}"
                f" -x-> {os.path.relpath(target, args.src)}"
            )
        return 0

    if args.fn:
        if ":" not in args.fn:
            print("--fn takes FILE:NAME, e.g. state/vault.zig:loadMetadata", file=sys.stderr)
            return 2
        rel, fn_name = args.fn.rsplit(":", 1)
        target = os.path.normpath(os.path.join(args.src, rel))
        if target not in files:
            print(f"no such source file: {rel}", file=sys.stderr)
            return 2
        classes = class_map(args.src, files, edges)
        hits = callers_of(files, target, fn_name)
        if not hits:
            print(
                f"{rel}:{fn_name}: no caller found. Either nothing calls it, or it is "
                "reached by a shape this tool cannot see — read the call sites before "
                "trusting a proof either way."
            )
            return 0
        print(f"{rel}:{fn_name} is called by")
        for caller in sorted(hits, key=lambda f: (classes[f], f)):
            print(f"  {classes[caller]:<10}  {os.path.relpath(caller, args.src)}")
        leak = [c for c in hits if classes[c] in LEAK_CAPABLE]
        if leak:
            print(f"\nLEAK-CAPABLE: {len(leak)} of {len(hits)} callers outlive a request.")
        else:
            print(
                "\nARENA-BACKED: every caller is under the per-request arena. "
                "Proving these rungs is the cosmetic work this sweep excludes."
            )
        return 0

    if args.why:
        target = os.path.normpath(os.path.join(args.src, args.why))
        if target not in files:
            print(f"no such source file: {args.why}", file=sys.stderr)
            return 2
        chain = shortest_path(edges, repeating_roots, target, frozenset(handlers))
        label = CLASS_REPEATING
        if chain is None:
            chain = shortest_path(edges, boot_roots, target, frozenset(handlers))
            label = CLASS_BOOT_ONCE
        if chain is None:
            print(f"{args.why}: no long-lived root reaches this file — every rung in it is arena-backed")
            return 0
        print(f"{args.why}: {label}, reached by")
        for depth, node in enumerate(chain):
            print(f"  {'  ' * depth}{os.path.relpath(node, args.src)}")
        print(
            "\nThe FILE is reachable. Confirm the FUNCTION you are proving is called on "
            "this chain — `--fn <file>:<name>` answers that directly."
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
