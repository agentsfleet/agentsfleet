#!/usr/bin/env python3
"""Production-only @import closure for Zig.

Walks @import("...") edges from the production entry points (src/main.zig and
src/executor/main.zig) AFTER stripping every `test "..." { ... }` and
`test { ... }` block (full nested-brace match). Emits any *.zig under src/
that is not in the resulting closure.

Test files, harnesses, fixtures, and zbench micro-benchmarks are exempt from
candidacy — they exist outside the production graph by design.

Exit 0 = no production orphans. Exit 1 = orphans found. Exit 2 = misconfig
(no source files discovered — likely wrong CWD).
"""
import os
import re
import sys

# Anchor paths to the repo root so the script behaves identically no matter
# where it's invoked from (e.g. `cd scripts && python audit-orphan-prod.py`
# would otherwise scan an empty tree and silently exit 0).
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROOT = os.path.join(REPO_ROOT, "src")
ENTRIES = [
    os.path.join(REPO_ROOT, "src/main.zig"),
    os.path.join(REPO_ROOT, "src/executor/main.zig"),
]

all_files = set()
for dp, dn, fn in os.walk(ROOT):
    if "/vendor" in dp or "/.zig-cache" in dp or "/third_party" in dp:
        continue
    for f in fn:
        if f.endswith(".zig"):
            all_files.add(os.path.join(dp, f))

if not all_files:
    print(f"audit-orphan-prod: no .zig files found under {ROOT}", file=sys.stderr)
    sys.exit(2)

imp_re = re.compile(r'@import\("([^"]+\.zig)"\)')


def strip_test_blocks(s: str) -> str:
    out, i = [], 0
    while i < len(s):
        m = re.match(r'\btest\b\s*("[^"]*"\s*)?\{', s[i:])
        if m:
            j = i + m.end()
            depth = 1
            while j < len(s) and depth > 0:
                if s[j] == "{":
                    depth += 1
                elif s[j] == "}":
                    depth -= 1
                j += 1
            i = j
            continue
        out.append(s[i])
        i += 1
    return "".join(out)


def imports(path):
    try:
        with open(path) as fh:
            src = fh.read()
    except OSError:
        return []
    src = strip_test_blocks(src)
    base = os.path.dirname(path)
    out = []
    for m in imp_re.findall(src):
        cand = os.path.normpath(os.path.join(base, m))
        if cand in all_files:
            out.append(cand)
    return out


seen = set(ENTRIES)
stack = list(ENTRIES)
while stack:
    f = stack.pop()
    for n in imports(f):
        if n not in seen:
            seen.add(n)
            stack.append(n)


def is_exempt(rel: str) -> bool:
    return (
        rel.endswith("_test.zig")
        or "/test_" in rel
        or rel.endswith("test_harness.zig")
        or rel.endswith("test_port.zig")
        or "fixture" in rel.lower()
        or "harness" in rel.lower()
        or rel.startswith("src/zbench")
        or rel == "src/auth/tests.zig"
    )


orphans = sorted(
    os.path.relpath(f, REPO_ROOT) for f in all_files - seen
)
orphans = [o for o in orphans if not is_exempt(o)]
for o in orphans:
    print(o)
sys.exit(1 if orphans else 0)
