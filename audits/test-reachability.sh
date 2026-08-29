#!/usr/bin/env bash
# test-reachability.sh — every Rust test file compiles into some binary.
#
# A crate with `autotests = false` compiles only the files a `[[test]]` target
# names, plus whatever those pull in with `#[path]` or a plain `mod`. A file
# listed nowhere is not a skipped test — it is not a test at all: it compiles
# in no binary, appears in no count, and fails nothing. Nobody notices, because
# a suite that never runs never goes red.
#
# This has happened twice. Aggregating a crate orphaned files that had been
# auto-discovered, and a later merge dropped eight sslmode tests into a crate
# whose aggregator did not know about them. Both were silent.
#
# Reports every `tests/*.rs` holding a `#[test]` or `#[tokio::test]` that no
# declared target reaches. Support modules with no tests are fine and ignored.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT/rustd" 2>/dev/null || { echo "OK:   test-reachability: no rustd workspace"; exit 0; }

python3 - <<'PY'
import pathlib, re, sys, tomllib

orphans = []
for crate in sorted(pathlib.Path("crates").iterdir()):
    manifest = crate / "Cargo.toml"
    if not manifest.exists():
        continue
    package = tomllib.loads(manifest.read_text())
    # autotests defaults to true, and cargo then discovers every tests/*.rs
    # by itself; only the opt-out can lose a file.
    if package.get("package", {}).get("autotests", True):
        continue
    tests = crate / "tests"
    if not tests.exists():
        continue

    declared = [t.get("path", "").split("/")[-1] for t in package.get("test", [])]
    reachable, frontier = set(declared), list(declared)
    while frontier:
        current = tests / frontier.pop()
        if not current.exists():
            continue
        source = current.read_text(errors="ignore")
        names = [m.group(1).split("/")[-1] for m in re.finditer(r'#\[path\s*=\s*"([^"]+)"\]', source)]
        names += [m.group(1) + ".rs" for m in re.finditer(r"^\s*mod\s+(\w+);", source, re.M)]
        for name in names:
            if name not in reachable:
                reachable.add(name)
                frontier.append(name)

    for found in sorted({f.name for f in tests.glob("*.rs")} - reachable):
        count = len(re.findall(r"#\[(?:tokio::)?test", (tests / found).read_text(errors="ignore")))
        if count:
            orphans.append((f"rustd/{crate.name}/tests/{found}", count))

if orphans:
    print("FAIL: test-reachability — test files no binary compiles")
    for path, count in orphans:
        print(f"  {path}  ({count} test fn) — declared by no [[test]] target and no #[path]")
    print("  Declare each in the crate's *_suite.rs aggregator, or give it its own [[test]].")
    sys.exit(1)

print("OK:   test-reachability: every test file compiles into a binary")
PY
