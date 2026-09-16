#!/usr/bin/env python3
"""Grade the archived evidence a datastore change is accepted on.

    CHECK=prototype|durability|retention|coordination|cluster \\
      python3 scripts/bench_datastore.py

Every acceptance row R1-R5 of the datastore spec is a call to this script, and
each mode names the evidence files that row is a claim about. The grader reads
`bench/results/`, never a datastore: the run happened earlier, the file is what
survives it, and a rubric that re-ran the measurement would be measuring the
machine it grades on.

WHY A GRADER AND NOT A GREP. Evidence fails in ways a person skimming numbers
does not see. A result copied from another branch reads perfectly. A run
against a shared endpoint reads perfectly and means nothing, because somebody
else's traffic is in the numbers. A run that leaked half its fixtures reads
perfectly and poisoned the population the NEXT run measured. A run that aborted
mid-window reads perfectly for the seconds it lasted. So four checks run on
every file in every mode before any measurement is looked at:

  1. present            — a missing file is a row nobody ran, not a row that
                          passed. This is the whole reason the modes name their
                          files instead of grading whatever is in the directory.
  2. described          — revision and datastore image non-empty, so the file
                          can be attributed to a build and a server. Written by
                          `afd_bench::report::Provenance`, which refuses to let
                          a lane run without them.
  3. owned              — the run had its target to itself. Invariant 7: only
                          owned datastores receive resets and fault injection,
                          and a shared endpoint's numbers are not this
                          deployment's.
  4. swept what it made — `fixture.created == fixture.swept`. A leaked fixture
                          is the failure that grades GREEN and breaks the next
                          run, which is exactly the shape this file exists for.

and a fifth, `abort`, which the lane writes when it stopped early.

Exit 0 when every required file passes, 1 with one line per violation.
"""
import json
import os
import sys
from pathlib import Path

RESULTS = Path("bench/results")

# The variable the make target passes, named once.
CHECK_VARIABLE = "CHECK"

# The profile a mode grades when the caller does not say. The rig is the only
# profile whose evidence is owned by construction.
PROFILE_VARIABLE = "PROFILE"
DEFAULT_PROFILE = "rig"

# What each acceptance row is a claim about.
#
# A mode is its required evidence plus the measurements that must hold in it.
# Naming the files is the point: a mode that graded "every file present" would
# pass a directory holding one lane's result and call it coverage.
#
# Budget entries read `(measurement, comparison, bound)`. The comparison is a
# word rather than an operator so a reader of the table knows which direction
# is failure without reversing an inequality in their head.
MODES = {
    # R1. The four cluster prototypes of §0, each its own evidence file: a
    # sharded subscription that survives slot movement, single-key primitives
    # across a migration, a ledger-first identity that survives queue loss, and
    # partitioned readiness under a hot partition.
    "prototype": {
        "files": ["prototype"],
        "budgets": [
            ("proofs", "at_least", 4.0),
            ("failures", "at_most", 0.0),
        ],
    },
    # R2. Accepted work survives the loss of the in-memory datastore, and
    # survives it exactly once.
    "durability": {
        "files": ["durability"],
        "budgets": [
            ("missing_admissions", "at_most", 0.0),
            ("duplicate_settlements", "at_most", 0.0),
            ("duplicate_debits", "at_most", 0.0),
        ],
    },
    # R3. Retention is bounded below by unfinished work and above by the
    # acknowledged-history cap, and a full datastore refuses rather than drops.
    "retention": {
        "files": ["outbound", "cardinality"],
        "budgets": [
            ("error_rate", "at_most", 0.0),
            ("silent_drops", "at_most", 0.0),
        ],
    },
    # R4. Discovery and delivery stay fair: no poll reads past its candidate
    # budget, and no unrelated work waits behind a slow destination.
    "coordination": {
        "files": ["lease", "steer"],
        "budgets": [
            ("error_rate", "at_most", 0.0),
        ],
    },
    # R5. Every suite has local-cluster evidence. The broadest row, and the one
    # a missing file is most likely to slip through, so it names all of them.
    "cluster": {
        "files": ["prototype", "durability", "lease", "steer", "outbound", "cardinality"],
        "budgets": [],
    },
}

# Read on every file in every mode, before a single measurement is.
REQUIRED_PROVENANCE = ("revision", "datastore_image")


def comparison_failed(measured, comparison, bound):
    """Whether `measured` breaks `bound`, in the direction `comparison` names."""
    if comparison == "at_most":
        return measured > bound
    if comparison == "at_least":
        return measured < bound
    raise ValueError(f"unknown comparison {comparison!r}")


def grade_provenance(name, report):
    """The four universal checks, as violation lines."""
    violations = []
    provenance = report.get("provenance")
    if not isinstance(provenance, dict):
        return [
            f"{name}: no provenance block — the run predates the field, or the "
            f"file was hand-written"
        ]
    for field in REQUIRED_PROVENANCE:
        if not str(provenance.get(field, "")).strip():
            violations.append(f"{name}: provenance.{field} is empty")
    if provenance.get("owned") is not True:
        violations.append(
            f"{name}: ran against a target it did not own — a shared endpoint's "
            f"numbers are not this deployment's (invariant 7)"
        )

    fixture = report.get("fixture") or {}
    created = fixture.get("created", 0)
    swept = fixture.get("swept", 0)
    if created != swept:
        violations.append(
            f"{name}: leaked {created - swept} of {created} fixtures "
            f"(created={created} swept={swept}) — the next run measures them too"
        )

    abort = report.get("abort")
    if abort:
        violations.append(f"{name}: run aborted early: {abort}")
    return violations


def grade_budgets(name, report, budgets):
    """The mode's own assertions, as violation lines."""
    violations = []
    measurements = report.get("measurements") or {}
    for measurement, comparison, bound in budgets:
        if measurement not in measurements:
            violations.append(
                f"{name}: no {measurement} measured — the file cannot answer "
                f"the row it was archived for"
            )
            continue
        measured = measurements[measurement]
        if comparison_failed(measured, comparison, bound):
            violations.append(
                f"{name}: {measurement}={measured} breaks {comparison} {bound}"
            )
    return violations


def grade(check, profile, results=RESULTS):
    """Every violation in `check`, in the order a reader would find them."""
    mode = MODES.get(check)
    if mode is None:
        expected = " | ".join(sorted(MODES))
        return [f"unknown {CHECK_VARIABLE}={check!r}: expected one of {expected}"]

    violations = []
    for stem in mode["files"]:
        name = f"{stem}.{profile}.json"
        path = results / name
        if not path.is_file():
            violations.append(
                f"{name}: missing from {results}/ — a row nobody ran is not a "
                f"row that passed"
            )
            continue
        try:
            report = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as failure:
            violations.append(f"{name}: unreadable as a result file: {failure}")
            continue
        violations.extend(grade_provenance(name, report))
        violations.extend(grade_budgets(name, report, mode["budgets"]))
    return violations


def main():
    check = os.environ.get(CHECK_VARIABLE, "").strip()
    if not check:
        expected = " | ".join(sorted(MODES))
        print(
            f"{CHECK_VARIABLE} is unset: expected {CHECK_VARIABLE}=<{expected}>",
            file=sys.stderr,
        )
        return 1
    profile = os.environ.get(PROFILE_VARIABLE, "").strip() or DEFAULT_PROFILE

    violations = grade(check, profile)
    if violations:
        print(f"✗ [bench-datastore] {check}: {len(violations)} violation(s)")
        for violation in violations:
            print(f"  {violation}")
        return 1
    graded = len(MODES[check]["files"])
    print(f"✓ [bench-datastore] {check}: {graded} evidence file(s) graded, profile={profile}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
