#!/usr/bin/env python3
"""Production-only @import closure for Zig.

Walks @import("...") edges from the production entry points (src/main.zig and
src/executor/main.zig) AFTER stripping every `test "..." { ... }` and
`test { ... }` block (depth-1 brace match). Emits any *.zig under src/ that
is not in the resulting closure.

Test files, harnesses, fixtures, and zbench micro-benchmarks are exempt from
candidacy — they exist outside the production graph by design.

Exit 0 = no production orphans. Exit 1 = orphans found (printed to stdout).
"""
import os
import re
import sys

ROOT = "src"
ENTRIES = ["src/main.zig", "src/executor/main.zig"]

all_files = set()
for dp, dn, fn in os.walk(ROOT):
    if "/vendor" in dp or "/.zig-cache" in dp or "/third_party" in dp:
        continue
    for f in fn:
        if f.endswith(".zig"):
            all_files.add(os.path.join(dp, f))

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
        src = open(path).read()
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


def is_exempt(f: str) -> bool:
    return (
        f.endswith("_test.zig")
        or "/test_" in f
        or f.endswith("test_harness.zig")
        or f.endswith("test_port.zig")
        or "fixture" in f.lower()
        or "harness" in f.lower()
        or f.startswith("src/zbench")
        or f == "src/auth/tests.zig"
    )


orphans = sorted(f for f in all_files - seen if not is_exempt(f))
for o in orphans:
    print(o)
sys.exit(1 if orphans else 0)
