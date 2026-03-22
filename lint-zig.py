#!/usr/bin/env python3
"""
check-pg-drain: verify every conn.query() call in Zig source has a .drain()
call in the same function block.

Rule from docs/ZIG_RULES.md:
  - Use conn.exec() for INSERT/UPDATE/DDL whenever possible.
  - Drain early-exit conn.query() results before deinit().

Exit 0 = all clear
Exit 1 = violations found

Suppress a specific function with a comment anywhere in the function body:
  // check-pg-drain: ok — <reason>
"""
import sys
import re
from pathlib import Path


def find_zig_files(root: str):
    return list(Path(root).rglob("*.zig"))


def extract_functions(text: str):
    """Yield (line_no, fn_name, fn_body) tuples for each fn in text."""
    lines = text.splitlines(keepends=True)
    fn_pattern = re.compile(r"^\s*(pub\s+)?fn\s+(\w+)")
    starts = []
    for i, line in enumerate(lines):
        if fn_pattern.match(line):
            starts.append(i)

    for idx, start in enumerate(starts):
        end = starts[idx + 1] if idx + 1 < len(starts) else len(lines)
        body = "".join(lines[start:end])
        m = fn_pattern.match(lines[start])
        fn_name = m.group(2) if m else "?"
        yield start + 1, fn_name, body


def check_file(path: Path):
    try:
        text = path.read_text()
    except Exception:
        return []

    errors = []
    for lineno, fn_name, body in extract_functions(text):
        if "conn.query(" not in body:
            continue
        if ".drain(" in body:
            continue
        if "// check-pg-drain: ok" in body:
            continue
        errors.append(
            f"  {path}:{lineno}: fn {fn_name} — conn.query() without .drain()"
        )

    return errors


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "src"
    files = find_zig_files(root)

    all_errors = []
    for f in sorted(files):
        all_errors.extend(check_file(f))

    if all_errors:
        print("FAIL pg-drain check — conn.query() without .drain():")
        for e in all_errors:
            print(e)
        print(f"\n{len(all_errors)} violation(s) found.")
        print(
            "Fix: add 'try result.drain();' or 'result.drain() catch {};' before deinit()."
        )
        print(
            "Alt: use conn.exec() for DDL/INSERT/UPDATE — it handles drain internally."
        )
        print("See docs/ZIG_RULES.md for the full query lifecycle rules.")
        print(
            "Suppress a false positive with: // check-pg-drain: ok — <reason>"
        )
        sys.exit(1)
    else:
        print(f"✓ pg-drain check passed ({len(files)} files scanned)")


if __name__ == "__main__":
    main()
