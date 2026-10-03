#!/usr/bin/env python3
"""Merge the Rust lane's lcov reports and grade the line floor once.

The integration lane runs as shards, each on a runner with its own datastores
(make/test-integration-rustd.mk, "Shards"). A shard measures only the tests it
ran, so no shard can be graded against the floor on its own: a third of the
suite covers far less than 97.5% of the lines. The floor is a claim about the
whole lane, so it is graded here, once, over the union of every shard.

A line is covered when any shard covered it. Hit counts are summed per line, the
rule Codecov already applies when it merges uploads, so the number this prints
and the number Codecov shows are computed the same way.

It refuses, rather than grading a partial union:
  - a report named on the command line that does not exist (a shard that never
    reported is not a shard that covered nothing),
  - a report whose `.rev` sidecar names another commit than --revision,
  - a union holding no lines at all,
  - a record type it does not merge, which would otherwise be dropped silently.

With --patch-base it also grades the patch: the lines `git diff <base> HEAD`
adds that llvm-cov instruments, against --patch-floor. That is Codecov's
`rust-afd` patch status (codecov.yml), graded here as well so the lane's verdict
does not wait on, or depend on, an asynchronous status nobody requires.

Exit 0 when every floor holds, 1 when one does not, 2 when the inputs are unusable.
"""
import argparse
import re
import subprocess
import sys
from decimal import Decimal, InvalidOperation
from pathlib import Path

EXIT_FLOOR_MISSED = 1
EXIT_UNUSABLE = 2
ROLLUP_ROWS = 12
CRATES_SEGMENT = "/crates/"
WORKSPACE_ROOT = "(workspace root)"
REVISION_SUFFIX = ".rev"
NOT_TAKEN = "-"
NEW_FILE = "+++ "
NEW_PATH = "+++ b/"
HUNK = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")
UNHIT_SHOWN = 20

# Totals the merge recomputes from the records it keeps; the inputs' own values
# describe one shard and are wrong for the union.
RECOMPUTED = frozenset({"TN", "LF", "LH", "FNF", "FNH", "BRF", "BRH"})


class UnusableReport(Exception):
    """An input the merge refuses to grade."""


class SourceFile:
    """One `SF:` record's counts, merged across every report that names it."""

    def __init__(self):
        self.lines = {}
        self.functions = {}
        self.function_hits = {}
        self.branches = {}

    def add_line(self, line, hits):
        self.lines[line] = self.lines.get(line, 0) + hits

    def add_branch(self, key, taken):
        # `-` means the block never ran, which is not a count of zero; it stays
        # `-` only while every report agrees.
        known = self.branches.get(key)
        if taken is None:
            self.branches.setdefault(key, None)
        else:
            self.branches[key] = (known or 0) + taken


def _number(text, where):
    try:
        return int(text)
    except ValueError:
        raise UnusableReport(f"{where}: {text!r} is not a count") from None


def parse(text, merged, origin):
    """Folds one lcov report into `merged`, keyed by source path."""
    current = None
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip()
        where = f"{origin}:{number}"
        if not line:
            continue
        if line == "end_of_record":
            current = None
            continue
        kind, _, body = line.partition(":")
        if kind in RECOMPUTED:
            continue
        if kind == "SF":
            current = merged.setdefault(body, SourceFile())
            continue
        if current is None:
            raise UnusableReport(f"{where}: {kind} outside a source file")
        _fold(current, kind, body.split(","), where)


def _fold(source, kind, fields, where):
    if kind == "DA" and len(fields) >= 2:
        source.add_line(_number(fields[0], where), _number(fields[1], where))
    elif kind == "FN" and len(fields) >= 2:
        source.functions.setdefault(",".join(fields[1:]), _number(fields[0], where))
    elif kind == "FNDA" and len(fields) >= 2:
        name = ",".join(fields[1:])
        source.function_hits[name] = source.function_hits.get(name, 0) + _number(fields[0], where)
    elif kind == "BRDA" and len(fields) == 4:
        key = tuple(_number(field, where) for field in fields[:3])
        taken = None if fields[3] == NOT_TAKEN else _number(fields[3], where)
        source.add_branch(key, taken)
    else:
        raise UnusableReport(f"{where}: {kind} is not a record this merge keeps")


def render(merged):
    """The union as lcov, ordered so the same inputs always give the same bytes."""
    out = ["TN:"]
    for path in sorted(merged):
        source = merged[path]
        out.append(f"SF:{path}")
        functions = sorted(source.functions.items(), key=lambda item: (item[1], item[0]))
        out.extend(f"FN:{line},{name}" for name, line in functions)
        out.extend(f"FNDA:{source.function_hits.get(name, 0)},{name}" for name, _ in functions)
        out.append(f"FNF:{len(functions)}")
        out.append(f"FNH:{sum(1 for name, _ in functions if source.function_hits.get(name, 0) > 0)}")
        out.extend(f"DA:{line},{hits}" for line, hits in sorted(source.lines.items()))
        if source.branches:
            for key, taken in sorted(source.branches.items()):
                out.append(f"BRDA:{key[0]},{key[1]},{key[2]},{NOT_TAKEN if taken is None else taken}")
            out.append(f"BRF:{len(source.branches)}")
            out.append(f"BRH:{sum(1 for taken in source.branches.values() if taken)}")
        out.append(f"LF:{len(source.lines)}")
        out.append(f"LH:{sum(1 for hits in source.lines.values() if hits > 0)}")
        out.append("end_of_record")
    return "\n".join(out) + "\n"


def totals(merged):
    """(covered, total) lines across the union."""
    covered = sum(1 for source in merged.values() for hits in source.lines.values() if hits > 0)
    total = sum(len(source.lines) for source in merged.values())
    return covered, total


def missed_by_crate(merged):
    """Missed lines per crate, most first: the answer to "where" after "how much"."""
    missed = {}
    for path, source in merged.items():
        gap = sum(1 for hits in source.lines.values() if hits == 0)
        if gap:
            crate = path.split(CRATES_SEGMENT, 1)[1].split("/", 1)[0] if CRATES_SEGMENT in path else WORKSPACE_ROOT
            missed[crate] = missed.get(crate, 0) + gap
    return sorted(missed.items(), key=lambda item: (-item[1], item[0]))


def holds(covered, total, floor):
    """Whether covered/total meets `floor` percent, decided without float rounding."""
    return Decimal(covered) * 100 >= floor * Decimal(total)


def changed_lines(diff_text):
    """Added line numbers per repository path, from `git diff --unified=0`."""
    changed = {}
    path = None
    for line in diff_text.splitlines():
        if line.startswith(NEW_FILE):
            # `+++ /dev/null` is a deleted file: nothing it adds can be unhit.
            path = line[len(NEW_PATH):] if line.startswith(NEW_PATH) else None
            continue
        hunk = HUNK.match(line)
        if hunk and path is not None:
            start, count = int(hunk.group(1)), int(hunk.group(2) or "1")
            changed.setdefault(path, set()).update(range(start, start + count))
    return changed


def diff_against(base):
    """The Rust lines HEAD adds over `base`, as `changed_lines` reads them."""
    command = ["git", "diff", "--unified=0", "--no-color", "--no-ext-diff", base, "HEAD", "--", "*.rs"]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise UnusableReport(f"git diff {base} HEAD failed: {result.stderr.strip()}")
    return changed_lines(result.stdout)


def patch_grade(merged, changed):
    """(covered, total, unhit) over the changed lines llvm-cov instruments.

    A changed line llvm-cov does not instrument — a comment, a brace, a file
    outside the report — is not coverable, so it counts on neither side.
    """
    covered = total = 0
    unhit = []
    for relative, lines in sorted(changed.items()):
        source = next((merged[path] for path in sorted(merged) if path == relative or path.endswith("/" + relative)), None)
        if source is None:
            continue
        for line in sorted(lines):
            hits = source.lines.get(line)
            if hits is None:
                continue
            total += 1
            if hits > 0:
                covered += 1
            else:
                unhit.append(f"{relative}:{line}")
    return covered, total, unhit


def load(paths, revision):
    merged = {}
    for path in paths:
        if not path.is_file():
            raise UnusableReport(f"{path} is missing: a shard that never reported cannot be graded as covering nothing")
        if revision is not None:
            sidecar = path.with_suffix(REVISION_SUFFIX)
            stamped = sidecar.read_text(encoding="utf-8").strip() if sidecar.is_file() else "(no sidecar)"
            if stamped != revision:
                raise UnusableReport(f"{path} was measured at {stamped}, not {revision}")
        parse(path.read_text(encoding="utf-8"), merged, path)
    return merged


def report(covered, total, floor, merged, out):
    pct = f"{covered * 100 / total:.4f}"
    missed = total - covered
    if holds(covered, total, floor):
        print(f"✓ [rustd] line coverage {pct}% >= {floor}% floor — {covered} of {total} lines covered, {missed} missed")
        print(f"  report at {out}")
        return 0
    print(f"✗ [rustd] line coverage {pct}% < {floor}% floor — {covered} of {total} lines covered, {missed} missed")
    print("  the floor is a ratchet: write the tests. Lowering RUSTD_COVERAGE_FLOOR is the thing it exists to prevent.")
    print("  missed lines by crate:")
    for crate, gap in missed_by_crate(merged)[:ROLLUP_ROWS]:
        print(f"    {gap:6d}  {crate}")
    print(f"  report at {out}")
    return EXIT_FLOOR_MISSED


def report_patch(covered, total, unhit, floor):
    if total == 0:
        print("✓ [rustd] patch coverage: the diff changes no line llvm-cov instruments")
        return 0
    pct = f"{covered * 100 / total:.4f}"
    if holds(covered, total, floor):
        print(f"✓ [rustd] patch coverage {pct}% >= {floor}% floor — {covered} of {total} changed lines covered")
        return 0
    print(f"✗ [rustd] patch coverage {pct}% < {floor}% floor — {covered} of {total} changed lines covered; unhit:")
    for where in unhit[:UNHIT_SHOWN]:
        print(f"    {where}")
    if len(unhit) > UNHIT_SHOWN:
        print(f"    … and {len(unhit) - UNHIT_SHOWN} more")
    return EXIT_FLOOR_MISSED


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--floor", required=True, help="line floor, in percent")
    parser.add_argument("--out", required=True, type=Path, help="where the merged lcov is written")
    parser.add_argument("--revision", help="the commit every report must carry in its .rev sidecar")
    parser.add_argument("--patch-base", help="grade the lines HEAD adds over this revision")
    parser.add_argument("--patch-floor", default="99", help="patch floor, in percent (codecov.yml's rust-afd target)")
    parser.add_argument("reports", nargs="+", type=Path)
    args = parser.parse_args(argv)
    try:
        floor, patch_floor = Decimal(args.floor), Decimal(args.patch_floor)
    except InvalidOperation:
        print(f"✗ [rustd] --floor {args.floor!r} / --patch-floor {args.patch_floor!r}: not a number", file=sys.stderr)
        return EXIT_UNUSABLE
    try:
        merged = load(args.reports, args.revision)
        covered, total = totals(merged)
        if total == 0:
            raise UnusableReport("the reports hold no lines: nothing was measured")
        changed = diff_against(args.patch_base) if args.patch_base else None
    except (UnusableReport, OSError) as failure:
        print(f"✗ [rustd] coverage merge refused: {failure}", file=sys.stderr)
        return EXIT_UNUSABLE
    args.out.write_text(render(merged), encoding="utf-8")
    verdict = report(covered, total, floor, merged, args.out)
    if changed is not None:
        verdict = max(verdict, report_patch(*patch_grade(merged, changed), patch_floor))
    return verdict


if __name__ == "__main__":
    sys.exit(main())
