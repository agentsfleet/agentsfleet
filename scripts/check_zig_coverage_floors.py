#!/usr/bin/env python3
"""Per-scope floor and target grading for the Zig coverage gate.

The gate used to enforce one merged percentage. Three trees move independently
underneath it — `agentsfleetd/`, `runner/` and `lib/` — so an average lets any
one of them fall while the published number holds steady. This grades each
scope on its own floor and publishes how far each still is from its target.

Floors are enforced and ratchet upward with evidence. Targets are fixed and
only change when a human changes them; they are published, never enforced, so
the distance to the destination stays visible without turning an unmet target
into a red build.

Lives beside `check_zig_coverage.py` rather than inside it: that module is at
its length cap, and floor grading is a separable concern with its own tests.
"""

from __future__ import annotations

from dataclasses import dataclass

# The scope name for the whole union. Folder scopes are named after the
# directory under `src/`, so this cannot collide with one.
MERGED_SCOPE = "merged"

# Path prefix every product folder sits under. A report path arrives
# repo-relative (`src/agentsfleetd/http/...`), so the folder is the second
# component.
SOURCE_PREFIX = "src/"


# kcov already drops `*_test.zig` via --exclude-pattern. Test roots reach the
# report under a different spelling, so they are dropped for the same reason: a
# gate satisfiable by writing more test files measures the wrong thing.
TEST_ROOT_NAMES = frozenset({"tests.zig", "test.zig"})

# Test-support sources carry neither a `_test.zig` suffix nor a test-root name,
# so both filters above miss them and they were counted as product. Harness is
# not shipped code, and at a 95% target its ~490 lines are wider than the margin
# being defended. Enumerated rather than matched on a broad `test` substring: a
# substring rule would also swallow product files whose names contain the word.
TEST_SUPPORT_NAMES = frozenset(
    {
        "test_harness.zig",
        "test_harness_server.zig",
        "test_fixtures.zig",
        "test_support.zig",
        "testing.zig",
        "test_sse_client.zig",
        "test_port.zig",
        "webhook_test_signers.zig",
    }
)

# Suffixes shared by whole families of fixture modules, where enumerating every
# spelling would go stale the next time one is added.
TEST_SUPPORT_SUFFIXES = ("_test_fixtures.zig", "_test_harness.zig", "_test_support.zig")


class UsageError(ValueError):
    """An argument combination the caller got wrong, not a coverage failure."""


def is_product_source(filename: str) -> bool:
    """True when a reported path is shipped code rather than a test body.

    Product helpers keep their place in the denominator even when their name
    reads test-adjacent — only the enumerated support forms leave, so
    `fleet_runtime/config_helpers.zig` and its siblings are still measured.
    """
    basename = filename.rsplit("/", 1)[-1]
    return not (
        basename.endswith("_test.zig")
        or basename in TEST_ROOT_NAMES
        or basename in TEST_SUPPORT_NAMES
        or basename.endswith(TEST_SUPPORT_SUFFIXES)
    )


@dataclass(frozen=True)
class Scope:
    """One graded scope: its measurement, its enforced floor, its target."""

    name: str
    files: int
    covered: int
    valid: int
    measured: float
    floor: float
    target: float

    @property
    def gap(self) -> float:
        """Points still to climb before the target is met; 0 once it is."""
        return max(0.0, self.target - self.measured)

    @property
    def breached(self) -> bool:
        # The epsilon matches the merged comparison in check_zig_coverage.py:
        # a rate that prints equal to its floor must not fail on binary
        # representation alone.
        return self.measured + 1e-9 < self.floor


def parse_scope_pct(values: list[str], flag: str) -> dict[str, float]:
    """Parse repeated `NAME=PCT` arguments into a mapping.

    Raises `UsageError` naming the flag and the offending value, so a typo in a
    make variable fails with the argument that caused it rather than a
    traceback.
    """
    parsed: dict[str, float] = {}
    for value in values:
        name, separator, raw = value.partition("=")
        if not separator or not name:
            raise UsageError(f"{flag} expects NAME=PCT, got {value!r}")
        try:
            parsed[name] = float(raw)
        except ValueError as error:
            raise UsageError(f"{flag} value for {name!r} is not a number: {raw!r}") from error
    return parsed


def folder_of(filename: str) -> str | None:
    """The product folder a repo-relative path belongs to, or None.

    `src/agentsfleetd/http/handlers/x.zig` → `agentsfleetd`. A path outside
    `src/` belongs to no graded folder and is counted only in the union.
    """
    if not filename.startswith(SOURCE_PREFIX):
        return None
    remainder = filename[len(SOURCE_PREFIX) :]
    folder, separator, _ = remainder.partition("/")
    return folder if separator else None


def folder_totals(merged: dict[tuple[str, int], bool]) -> dict[str, tuple[int, int, int]]:
    """Return {folder: (files, covered, valid)} over the merged union."""
    seen_files: dict[str, set[str]] = {}
    covered: dict[str, int] = {}
    valid: dict[str, int] = {}
    for (filename, _number), hit in merged.items():
        folder = folder_of(filename)
        if folder is None:
            continue
        seen_files.setdefault(folder, set()).add(filename)
        valid[folder] = valid.get(folder, 0) + 1
        if hit:
            covered[folder] = covered.get(folder, 0) + 1
    return {
        folder: (len(files), covered.get(folder, 0), valid.get(folder, 0))
        for folder, files in seen_files.items()
    }


def build_scope(
    name: str, files: int, covered: int, valid: int, floors: dict[str, float], targets: dict[str, float]
) -> Scope:
    """Assemble one scope, defaulting an unset floor or target to zero.

    A floor above its own target is a usage error: the ratchet would be set
    past its destination, which is always an editing mistake rather than a
    coverage outcome.
    """
    floor = floors.get(name, 0.0)
    target = targets.get(name, 0.0)
    if target and floor > target:
        raise UsageError(
            f"floor for {name!r} is {floor:g}%, above its own target {target:g}% — "
            "the ratchet cannot be set past its destination"
        )
    measured = (covered / valid * 100) if valid else 0.0
    return Scope(
        name=name,
        files=files,
        covered=covered,
        valid=valid,
        measured=measured,
        floor=floor,
        target=target,
    )


def build_scopes(
    merged: dict[tuple[str, int], bool],
    union_summary: tuple[int, int, int],
    floors: dict[str, float],
    targets: dict[str, float],
) -> list[Scope]:
    """The merged scope followed by every product folder, in name order."""
    files, covered, valid = union_summary
    scopes = [build_scope(MERGED_SCOPE, files, covered, valid, floors, targets)]
    totals = folder_totals(merged)
    for folder in sorted(totals):
        folder_files, folder_covered, folder_valid = totals[folder]
        scopes.append(
            build_scope(folder, folder_files, folder_covered, folder_valid, floors, targets)
        )
    return scopes


def unknown_scope_names(scopes: list[Scope], *mappings: dict[str, float]) -> list[str]:
    """Names given a floor or target that no measured scope carries.

    A folder renamed in the tree but not in `make/test.mk` would otherwise have
    its floor silently ignored, which is the failure this catches.
    """
    known = {scope.name for scope in scopes}
    named: set[str] = set()
    for mapping in mappings:
        named.update(mapping)
    return sorted(named - known)


def breaches(scopes: list[Scope]) -> list[str]:
    """One message per scope below its floor, naming scope, measured and floor.

    The wording stays `is below threshold` so a folder breach and the merged
    breach read identically apart from the scope name — the operator learns one
    sentence, and the scope in front of it is the only thing that varies.
    """
    return [
        f"✗ {scope.name} line coverage {scope.measured:.2f}% is below threshold "
        f"{scope.floor:.2f}% ({scope.covered}/{scope.valid} lines across {scope.files} files)"
        for scope in scopes
        if scope.breached
    ]


def grade_denominator(
    merged: dict[tuple[str, int], bool],
    required_roots: list[str],
    union_counts: tuple[int, int],
    union_minimums: tuple[int, int],
) -> list[str]:
    """Every assertion about the report's shape, before any rate is compared.

    A rate graded over a denominator nobody checked is the failure this lane
    exists to stop, so these run first and an empty result is the only way
    through to the percentage comparison.
    """
    problems: list[str] = []
    shortfall = union_shortfall(*union_counts, *union_minimums)
    if shortfall is not None:
        problems.append(shortfall)
    absent = missing_roots(merged, required_roots)
    if absent:
        problems.append(
            "✗ union carries no measured line for required product root(s): " + ", ".join(absent)
        )
    return problems


def summary_keys(scopes: list[Scope]) -> str:
    """The key/value surface for every scope: rate, floor, target and gap.

    One emitter writes all four per scope. There is deliberately no path that
    publishes a rate without the floor and target that give it meaning — a bare
    percentage is the misreading this gate exists to stop.
    """
    lines: list[str] = []
    for scope in scopes:
        if scope.name == MERGED_SCOPE:
            lines.append(f"zig_line_coverage_target_pct={scope.target:g}")
            lines.append(f"zig_line_coverage_gap_pts={scope.gap:.2f}")
            continue
        lines.append(f"zig_folder_pct_{scope.name}={scope.measured:.2f}")
        lines.append(f"zig_folder_min_pct_{scope.name}={scope.floor:g}")
        lines.append(f"zig_folder_target_pct_{scope.name}={scope.target:g}")
        lines.append(f"zig_folder_gap_pts_{scope.name}={scope.gap:.2f}")
    return "".join(f"{line}\n" for line in lines)


def missing_roots(merged: dict[tuple[str, int], bool], required: list[str]) -> list[str]:
    """Product roots that carry no measured line in the union.

    Independent of rate: a union at 98% holding only one tree is not a
    measurement of the codebase, and no percentage can say so.
    """
    present = {folder_of(filename) for filename, _number in merged}
    return sorted(root for root in required if root not in present)


def union_shortfall(files: int, valid: int, min_files: int, min_lines: int) -> str | None:
    """The union's own denominator floor, or None when it holds."""
    if files >= min_files and valid >= min_lines:
        return None
    return (
        f"✗ union measured {files} files / {valid} lines, below its minimum of "
        f"{min_files} files / {min_lines} lines"
    )


def report_lines(scopes: list[Scope]) -> list[str]:
    """Human-readable per-scope rows for the gate's stdout."""
    return [
        f"  {scope.name:<14} {scope.measured:6.2f}%  floor {scope.floor:g}  "
        f"target {scope.target:g}  gap {scope.gap:.2f}  "
        f"({scope.covered}/{scope.valid} lines, {scope.files} files)"
        for scope in scopes
    ]
