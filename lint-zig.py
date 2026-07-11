#!/usr/bin/env python3
"""
lint-zig.py — deterministic Zig discipline checks.

Two modes:

  python3 lint-zig.py [ROOT]                       -> pg-drain check (default)
  python3 lint-zig.py --discipline [--roster P] ROOT
                                                    -> ghostty-derived A5/A2 checks

pg-drain: every conn.query() call has a .drain() in the same function block.
  Suppress with:  // check-pg-drain: ok — <reason>

discipline (rules A1-A6 / C1-C5 in dispatch/write_zig.md), roster-scoped by
audits/zig-discipline-roster.txt:
  A5-POISON  every freeing deinit ends with `self.* = undefined`
  A5-PHRASE  every owned-slice-returning pub fn states ownership in a fixed phrase
  A2-ERRDEFER (advisory) multi-`try` init with zero errdefer — never blocks
  Inside a roster prefix POISON/PHRASE BLOCK (exit 1); outside they WARN (exit 0).
  Suppress a specific finding with:  // discipline: ok — <reason>

Exit 0 = clear (advisory warnings do not fail); 1 = blocking violations found.
"""
import sys
import re
from pathlib import Path

# --- shared -----------------------------------------------------------------

FN_PATTERN = re.compile(r"^\s*(pub\s+)?fn\s+(\w+)")


def find_zig_files(root: str):
    return list(Path(root).rglob("*.zig"))


def extract_functions(text: str):
    """Yield (line_no, fn_name, fn_body, start_idx) for each fn in text."""
    lines = text.splitlines(keepends=True)
    starts = [i for i, line in enumerate(lines) if FN_PATTERN.match(line)]
    for idx, start in enumerate(starts):
        end = starts[idx + 1] if idx + 1 < len(starts) else len(lines)
        body = "".join(lines[start:end])
        m = FN_PATTERN.match(lines[start])
        fn_name = m.group(2) if m else "?"
        yield start + 1, fn_name, body, start


# --- pg-drain check (default mode) ------------------------------------------

DRAIN_SUPPRESS = "// check-pg-drain: ok"


def check_pg_drain(path: Path):
    try:
        text = path.read_text()
    except Exception:
        return []

    errors = []
    for lineno, fn_name, body, _ in extract_functions(text):
        if "conn.query(" not in body:
            continue
        if ".drain(" in body:
            continue
        # PgQuery wraps the result and auto-drains in deinit().
        if "PgQuery.from(" in body:
            continue
        if DRAIN_SUPPRESS in body:
            continue
        errors.append(
            f"  {path}:{lineno}: fn {fn_name} — conn.query() without .drain()"
        )
    return errors


def run_pg_drain(root: str) -> int:
    files = find_zig_files(root)
    all_errors = []
    for f in sorted(files):
        all_errors.extend(check_pg_drain(f))
    if all_errors:
        print("FAIL pg-drain check — conn.query() without .drain():")
        for e in all_errors:
            print(e)
        print(f"\n{len(all_errors)} violation(s) found.")
        print("Fix: add 'try result.drain();' or 'result.drain() catch {};' before deinit().")
        print("Alt: use conn.exec() for DDL/INSERT/UPDATE — it handles drain internally.")
        print("Suppress a false positive with: // check-pg-drain: ok — <reason>")
        return 1
    print(f"✓ pg-drain check passed ({len(files)} files scanned)")
    return 0


# --- discipline checks (ghostty-derived A5/A2) ------------------------------

DEFAULT_ROSTER = "audits/zig-discipline-roster.txt"
DISCIPLINE_SUPPRESS = "// discipline: ok"
POISON_STMT = "self.* = undefined"
OWNERSHIP_PHRASES = ("caller must free", "takes ownership", "caller owns", "caller frees")
# A deinit that touches one of these frees something → it carries a poison contract.
FREE_MARKERS = ("alloc.free(", "alloc.destroy(", ".destroy(", ".unref(", ".deinit(")
# Owned-slice return shapes: `![]const u8`, `![]u8`, optional variants, `Error!` prefix.
OWNED_RETURN = re.compile(r"!\??\[\]const u8|!\??\[\]u8")
# Init-shaped fn names for the advisory errdefer heuristic.
INIT_NAMES = ("init", "create", "open", "start", "connect", "build", "spawn")
# A `try` acquisition worth an errdefer.
TRY_ACQUIRE = re.compile(r"\btry\s+[\w.]*(alloc|dupe|create|init|clone|connect|open)\w*\s*\(")

RULE_POISON = "A5-POISON"
RULE_PHRASE = "A5-PHRASE"
RULE_ERRDEFER = "A2-ERRDEFER"


def load_roster(path: str):
    prefixes = []
    try:
        for line in Path(path).read_text().splitlines():
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            prefixes.append(s)
    except FileNotFoundError:
        pass
    return prefixes


def in_roster(path_str: str, prefixes) -> bool:
    p = str(path_str).replace("\\", "/")
    return any(p.startswith(pre) for pre in prefixes)


def _doc_above(lines, start_idx: int) -> str:
    """Contiguous /// or // comment lines immediately above a fn declaration."""
    out = []
    i = start_idx - 1
    while i >= 0:
        stripped = lines[i].lstrip()
        if stripped.startswith("///") or stripped.startswith("//"):
            out.append(lines[i])
            i -= 1
        else:
            break
    return "".join(out)


def _signature(body: str) -> str:
    return body.split("{", 1)[0]


def discipline_findings(path: Path):
    """Return (blocking_candidates, advisories) as lists of (lineno, rule, msg)."""
    try:
        text = path.read_text()
    except Exception:
        return [], []
    lines = text.splitlines(keepends=True)
    blocking, advisory = [], []

    for lineno, name, body, start_idx in extract_functions(text):
        sig = _signature(body)
        doc = _doc_above(lines, start_idx)

        # A5-POISON — a freeing pointer-receiver deinit must poison.
        if name == "deinit" and "self: *" in sig:
            frees = any(m in body for m in FREE_MARKERS)
            if frees and POISON_STMT not in body and DISCIPLINE_SUPPRESS not in body:
                blocking.append((lineno, RULE_POISON,
                                 f"deinit frees but omits `{POISON_STMT}` (rule A5)"))

        # A5-PHRASE — an owned-slice-returning pub fn must state ownership.
        if name != "deinit" and "pub fn" in sig and OWNED_RETURN.search(sig):
            haystack = (doc + body).lower()
            if not any(p in haystack for p in OWNERSHIP_PHRASES) and DISCIPLINE_SUPPRESS not in body:
                blocking.append((lineno, RULE_PHRASE,
                                 "owned-slice return without an ownership phrase "
                                 '("caller must free" / "takes ownership") (rule A5)'))

        # A2-ERRDEFER (advisory) — multi-try init with no errdefer.
        looks_init = name.startswith(INIT_NAMES) or "!*" in sig or "!Self" in sig
        if looks_init and "errdefer" not in body:
            if len(TRY_ACQUIRE.findall(body)) >= 2 and DISCIPLINE_SUPPRESS not in body:
                advisory.append((lineno, RULE_ERRDEFER,
                                 f"fn {name}: 2+ `try` acquisitions, no errdefer — "
                                 "verify partial-init cleanup (rule A2, advisory)"))

    return blocking, advisory


def run_discipline(root: str, roster_path: str, list_warnings: bool = False) -> int:
    prefixes = load_roster(roster_path)
    files = find_zig_files(root)
    blocking_hits, warn_hits = [], []

    for f in sorted(files):
        if "_test.zig" in f.name or f.name == "tests.zig":
            continue
        blocking, advisory = discipline_findings(f)
        bound = in_roster(f, prefixes)
        for lineno, rule, msg in blocking:
            line = f"{f}:{lineno}: {rule}: {msg}"
            (blocking_hits if bound else warn_hits).append(line)
        for lineno, rule, msg in advisory:
            warn_hits.append(f"{f}:{lineno}: {rule}: {msg}")

    # Out-of-roster + advisory findings are visible but non-blocking. Enumerate
    # them only under --list so a routine `make lint` is not flooded; the default
    # prints a one-line count so the discipline stays visible everywhere.
    if list_warnings:
        for w in warn_hits:
            print(f"⚠ warn  {w}")
    elif warn_hits:
        print(f"⚠ {len(warn_hits)} advisory / out-of-roster finding(s) "
              "(non-blocking) — rerun with --list to enumerate")

    if blocking_hits:
        print("\nFAIL discipline check — binding A5 violations inside the roster:")
        for b in blocking_hits:
            print(f"  {b}")
        print(f"\n{len(blocking_hits)} blocking violation(s) in the discipline base.")
        print("Fix: poison every freeing deinit with `self.* = undefined`; state ownership")
        print('     ("caller must free" / "takes ownership") on every owned-slice pub fn.')
        print("Roster: audits/zig-discipline-roster.txt. Suppress: // discipline: ok — <reason>")
        return 1

    print(f"✓ discipline check passed ({len(files)} files scanned, "
          f"{len(prefixes)} roster prefixes, {len(warn_hits)} advisory)")
    return 0


# --- entrypoint -------------------------------------------------------------

def main():
    argv = sys.argv[1:]
    discipline = "--discipline" in argv
    roster = DEFAULT_ROSTER
    if "--roster" in argv:
        ri = argv.index("--roster")
        roster = argv[ri + 1]
        del argv[ri:ri + 2]
    list_warnings = "--list" in argv
    positionals = [a for a in argv if not a.startswith("--")]
    root = positionals[0] if positionals else "src"

    if discipline:
        sys.exit(run_discipline(root, roster, list_warnings))
    sys.exit(run_pg_drain(root))


if __name__ == "__main__":
    main()
