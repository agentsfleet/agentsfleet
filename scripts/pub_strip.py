#!/usr/bin/env python3
"""pub_strip.py — remove `pub` from declarations listed in a TSV.

Input TSV format (matches scripts/pub_audit.sh output):
  file<TAB>kind<TAB>name<TAB>refs_outside_file

Only rows with refs_outside_file == "0" are processed; "TEST_ONLY" rows are
skipped (sibling _test.zig consumers need pub).

Usage:
  scripts/pub_strip.py --tsv /tmp/audit.tsv [--dir-filter src/cli/]

Exits non-zero if any candidate line cannot be matched (likely audit drift).
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tsv", required=True)
    ap.add_argument("--dir-filter", default="", help="Only process rows whose file starts with this prefix")
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    rows: dict[str, list[tuple[str, str]]] = defaultdict(list)
    with open(args.tsv) as f:
        for raw in f:
            parts = raw.rstrip("\n").split("\t")
            if len(parts) != 4:
                continue
            file, kind, name, refs = parts
            if refs != "0":
                continue
            if args.dir_filter and not file.startswith(args.dir_filter):
                continue
            rows[file].append((kind, name))

    total = 0
    failures: list[str] = []
    for file, items in sorted(rows.items()):
        path = Path(file)
        text = path.read_text()
        new = text
        local_count = 0
        for kind, name in items:
            kind_pat = re.escape(kind).replace(r"\ ", r"\s+")
            # Anchor: optional leading whitespace, then "pub <kind> <name>" with a non-word boundary after the name.
            pat = re.compile(
                rf"^([ \t]*)pub\s+({kind_pat})\s+({re.escape(name)})\b",
                re.MULTILINE,
            )
            replaced = pat.subn(r"\1\2 \3", new, count=1)
            if replaced[1] == 0:
                failures.append(f"{file}: could not strip pub from {kind} {name}")
            else:
                new = replaced[0]
                local_count += 1
                total += 1
        if local_count and not args.dry_run:
            path.write_text(new)
        print(f"{file}: stripped {local_count}/{len(items)}")

    print(f"\nTotal stripped: {total}")
    if failures:
        print("\nFailures:")
        for f in failures:
            print(f"  {f}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
