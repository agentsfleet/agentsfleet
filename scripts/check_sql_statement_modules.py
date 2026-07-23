#!/usr/bin/env python3
"""Gate statement-module adoption across the data-access layer.

Eleven domains already keep their SQL in a sibling `sql.zig`; the rest carry it
inline, which is why auditing the query surface means reading every module
rather than one per domain. This computes the adoption ratio and fails below a
threshold so the convention cannot quietly regress.

The denominator is deliberately NOT "every file containing SQL". A handler with
one statement gains nothing from a sibling module, and forcing it there is churn
without signal. What earns a module is a domain carrying enough statements that
having them in one place changes how you read it.

Exit 0 at or above the threshold, 1 below, 2 on unreadable input.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# Adoption floor. A domain-level convention is only worth gating if the gate
# actually holds; below this the ratio is noise rather than a rule.
DEFAULT_THRESHOLD_PCT = 80.0

# A domain needs at least this many statements before a sibling module pays for
# itself. One or two statements read fine where they are used.
MIN_STATEMENTS_PER_DOMAIN = 3

STATEMENT_MODULE_NAME = "sql.zig"

# Roots of the data-access layer. Everything else (transport, config, crypto
# primitives) is out of scope by construction, not by exclusion.
DATA_ACCESS_ROOTS = (
    "src/agentsfleetd/state",
    "src/agentsfleetd/fleet",
    "src/agentsfleetd/fleet_runtime",
    "src/agentsfleetd/memory",
    "src/agentsfleetd/cron",
    "src/agentsfleetd/fleet_library",
    "src/agentsfleetd/secrets",
    "src/agentsfleetd/http/handlers",
)

# Paths whose inline SQL is inline BY DESIGN, and must not drag the ratio down.
#
# - fixtures seed through the real schema and read best beside their assertions
# - the migration bootstrap must not depend on a module it may be creating
# - the two metering statements are the most correctness-critical text in the
#   repository and gain nothing from moving; see the spec's §5 note
EXCLUDED_SUFFIXES = (
    "db/test_fixtures.zig",
    "db/pool_migrations.zig",
    "db/migration_versions.zig",
    "fleet/renewal.zig",
    "fleet/renewal_settle.zig",
)

# A statement starts a Zig multiline-string line and opens with a SQL verb.
STATEMENT_RE = re.compile(r"^\s*\\\\\s*(SELECT|INSERT\s+INTO|UPDATE|DELETE\s+FROM|WITH)\b", re.IGNORECASE)

# A `test "…" {` / `test {` block inside a production module. Its SQL is fixture
# text, inline by design, and must not enter the denominator.
TEST_BLOCK_RE = re.compile(r"^test\b")

# A constant surface holds no function and allocates nothing.
FUNCTION_RE = re.compile(r"^\s*(pub\s+)?fn\s", re.MULTILINE)
ALLOC_RE = re.compile(r"\balloc(ator)?\b")


class CheckError(Exception):
    """Input could not be read or parsed."""


def is_excluded(path: Path) -> bool:
    posix = path.as_posix()
    if posix.endswith("_test.zig") or "/tests.zig" in posix:
        return True
    return any(posix.endswith(suffix) for suffix in EXCLUDED_SUFFIXES)


def in_data_access_layer(path: Path) -> bool:
    posix = path.as_posix()
    return any(root in posix for root in DATA_ACCESS_ROOTS)


def count_statements(path: Path) -> int:
    """Production statements in `path`, ignoring any inside a `test` block.

    A production module can hold `test { … }` blocks, and their fixture SQL is
    inline by design — the same reason `*_test.zig` is excluded wholesale. Left
    in, it inflates the denominator with text nobody should extract, which makes
    the ratio understate adoption and, worse, invites someone to "fix" it by
    moving fixtures into a production statement module.
    """
    try:
        source = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise CheckError(f"cannot read {path}: {exc}") from exc
    except UnicodeDecodeError as exc:
        raise CheckError(f"{path} is not valid UTF-8: {exc}") from exc

    count = 0
    depth = 0
    in_test = False
    for line in source.splitlines():
        if not in_test and TEST_BLOCK_RE.match(line):
            in_test, depth = True, 0
        if in_test:
            # Brace depth returning to zero closes the block. Braces inside the
            # SQL string itself are not Zig syntax, so skip continuation lines.
            if not line.lstrip().startswith("\\\\"):
                depth += line.count("{") - line.count("}")
                if depth <= 0:
                    in_test = False
            continue
        if STATEMENT_RE.match(line):
            count += 1
    return count


def domain_of(path: Path) -> Path:
    return path.parent


def survey(root: Path) -> dict[Path, dict[str, int]]:
    """Statements per domain directory, split by where the text lives."""
    domains: dict[Path, dict[str, int]] = {}
    for path in sorted(root.rglob("*.zig")):
        if not in_data_access_layer(path) or is_excluded(path):
            continue
        count = count_statements(path)
        if count == 0:
            continue
        bucket = domains.setdefault(domain_of(path), {"extracted": 0, "inline": 0})
        if path.name == STATEMENT_MODULE_NAME:
            bucket["extracted"] += count
        else:
            bucket["inline"] += count
    return domains


def adoption(domains: dict[Path, dict[str, int]]) -> tuple[int, int]:
    """(extracted, total) statements over domains large enough to qualify."""
    extracted = total = 0
    for counts in domains.values():
        domain_total = counts["extracted"] + counts["inline"]
        if domain_total < MIN_STATEMENTS_PER_DOMAIN:
            continue
        extracted += counts["extracted"]
        total += domain_total
    return extracted, total


def is_constant_surface(path: Path) -> bool:
    """A statement module declares constants only — no function, no allocation."""
    try:
        source = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise CheckError(f"cannot read {path}: {exc}") from exc
    body = "\n".join(line for line in source.splitlines() if not line.lstrip().startswith(("//", "\\\\")))
    return FUNCTION_RE.search(body) is None and ALLOC_RE.search(body) is None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default="src", type=Path)
    parser.add_argument("--threshold", default=DEFAULT_THRESHOLD_PCT, type=float)
    parser.add_argument("--verbose", action="store_true", help="list each domain's split")
    args = parser.parse_args()

    if not args.root.is_dir():
        print(f"✗ [sql-modules] {args.root} is not a directory", file=sys.stderr)
        return 2

    try:
        domains = survey(args.root)
        offenders = [
            path
            for path in sorted(args.root.rglob(STATEMENT_MODULE_NAME))
            if not is_constant_surface(path)
        ]
    except CheckError as exc:
        print(f"✗ [sql-modules] {exc}", file=sys.stderr)
        return 2

    for path in offenders:
        print(f"✗ [sql-modules] {path} is not a constant surface (holds a function or allocates)")

    extracted, total = adoption(domains)
    pct = 100.0 if total == 0 else (extracted / total) * 100.0
    print(f"{extracted}/{total} ({pct:.1f}%)")

    if args.verbose:
        for domain, counts in sorted(domains.items()):
            domain_total = counts["extracted"] + counts["inline"]
            if domain_total < MIN_STATEMENTS_PER_DOMAIN:
                continue
            print(f"  {domain}: {counts['extracted']} extracted, {counts['inline']} inline")

    if offenders:
        return 1
    if pct + 1e-9 < args.threshold:
        print(f"✗ [sql-modules] adoption {pct:.1f}% is below the {args.threshold:.1f}% floor")
        return 1
    print(f"✓ [sql-modules] adoption {pct:.1f}% meets the {args.threshold:.1f}% floor")
    return 0


if __name__ == "__main__":
    sys.exit(main())
