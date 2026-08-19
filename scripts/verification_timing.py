#!/usr/bin/env python3
"""Compare verification critical paths, and refuse samples that cannot be compared.

A speed claim is only as good as the pair it compares. The failure this exists
to prevent has already happened here once: a single 10m53-against-14m33 reading
taken under different source and cache conditions, reported as a 25% reduction
it could not support.

Three things have to hold before a median means anything:

  * every sample inside one arm comes from one commit — otherwise the arm is a
    mixture of workloads, not a measurement of one;
  * every sample shares a runner image — a different image is a different
    machine, and the whole comparison is about machines;
  * the two arms share a workload digest — the baseline and the candidate sit on
    different commits by construction, so what must match is the code under
    test, not the revision. A candidate that also changed a test would be
    measuring a different suite and calling it a speedup.

Cold and warm cache states are reported separately and never pooled. They are
different questions: cold is what a new branch pays, warm is what a push to an
existing one pays, and averaging them describes neither.

Samples come from central Continuous Integration (CI) job timestamps, not from
the runners' own clocks. Independently clocked workers cannot prove overlap.
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path

BASELINE = "baseline"
CANDIDATE = "candidate"
CACHE_STATES = ("cold", "warm")
REQUIRED_KEYS = ("arm", "cache_state", "commit", "image", "workload_digest", "seconds")
DEFAULT_MIN_SAMPLES = 3


def load_samples(path: Path) -> list[dict]:
    with path.open(encoding="utf-8") as handle:
        samples = json.load(handle)
    if not isinstance(samples, list) or not samples:
        raise ValueError(f"{path} does not hold a non-empty list of samples")
    for index, sample in enumerate(samples):
        missing = [key for key in REQUIRED_KEYS if key not in sample]
        if missing:
            raise ValueError(f"sample {index} is missing {', '.join(missing)}")
    return samples


def check_comparable(samples: list[dict]) -> list[str]:
    failures = []
    images = {sample["image"] for sample in samples}
    if len(images) > 1:
        failures.append(f"samples span more than one runner image: {sorted(images)}")
    digests = {sample["workload_digest"] for sample in samples}
    if len(digests) > 1:
        failures.append(
            "the arms measure different code under test — "
            f"workload_digest disagrees across {sorted(digests)}"
        )
    for arm in (BASELINE, CANDIDATE):
        commits = {sample["commit"] for sample in samples if sample["arm"] == arm}
        if len(commits) > 1:
            failures.append(f"{arm} samples span more than one commit: {sorted(commits)}")
        if not commits:
            failures.append(f"no {arm} samples")
    for state in {sample["cache_state"] for sample in samples}:
        if state not in CACHE_STATES:
            failures.append(f"unknown cache state {state!r}; expected one of {CACHE_STATES}")
    return failures


def group(samples: list[dict]) -> dict[tuple[str, str], list[float]]:
    grouped: dict[tuple[str, str], list[float]] = {}
    for sample in samples:
        key = (sample["cache_state"], sample["arm"])
        grouped.setdefault(key, []).append(float(sample["seconds"]))
    return grouped


def check_depth(grouped: dict, minimum: int) -> list[str]:
    failures = []
    for state in CACHE_STATES:
        for arm in (BASELINE, CANDIDATE):
            durations = grouped.get((state, arm), [])
            if not durations:
                failures.append(f"no {state} {arm} samples")
            elif len(durations) < minimum:
                failures.append(
                    f"{state} {arm} has {len(durations)} sample(s); {minimum} are required"
                )
    return failures


def report(grouped: dict) -> tuple[list[str], bool]:
    lines = ["cache    baseline    candidate      change"]
    improved = True
    for state in CACHE_STATES:
        base = statistics.median(grouped[(state, BASELINE)])
        cand = statistics.median(grouped[(state, CANDIDATE)])
        change = (base - cand) / base * 100.0
        improved = improved and change > 0
        lines.append(f"{state:<8} {base / 60:7.1f}m   {cand / 60:7.1f}m   {change:+6.1f}%")
    return lines, improved


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--samples", type=Path, required=True)
    parser.add_argument("--min-samples", type=int, default=DEFAULT_MIN_SAMPLES)
    args = parser.parse_args(argv)

    try:
        samples = load_samples(args.samples)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"✗ [timing] {error}", file=sys.stderr)
        return 1

    failures = check_comparable(samples)
    grouped = group(samples)
    failures.extend(check_depth(grouped, args.min_samples))
    if failures:
        for failure in failures:
            print(f"✗ [timing] {failure}", file=sys.stderr)
        return 1

    lines, improved = report(grouped)
    print("\n".join(lines))
    if not improved:
        print("✗ [timing] the candidate is not faster in every cache state", file=sys.stderr)
        return 1
    print("✓ [timing] the candidate improves the median critical path in both cache states")
    return 0


if __name__ == "__main__":
    sys.exit(main())
