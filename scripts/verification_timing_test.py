#!/usr/bin/env python3
"""Prove a speed claim cannot be made from samples that do not compare.

The reading this guards against has already been produced here once: one
candidate run against one baseline run, taken under different source and cache
conditions, reported as a percentage. Each test below builds a sample
set that is wrong in exactly one way and checks it is refused for that reason.
"""

from __future__ import annotations

import io
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

import verification_timing as timing

ROOT = Path(__file__).resolve().parents[1]
IMAGE = "ghcr.io/agentsfleet/ci-zig-ubuntu:0.16.0-r4"
WORKLOAD = "0f1e2d3c"
BASE_COMMIT = "b8fc99e7d"
HEAD_COMMIT = "311c97e22"


def sample(arm: str, cache_state: str, seconds: float, **overrides) -> dict:
    body = {
        "arm": arm,
        "cache_state": cache_state,
        "commit": BASE_COMMIT if arm == timing.BASELINE else HEAD_COMMIT,
        "image": IMAGE,
        "workload_digest": WORKLOAD,
        "seconds": seconds,
    }
    body.update(overrides)
    return body


def comparable_set() -> list[dict]:
    samples = []
    for state, base, cand in (("cold", 870.0, 660.0), ("warm", 690.0, 520.0)):
        for offset in (-12.0, 0.0, 15.0):
            samples.append(sample(timing.BASELINE, state, base + offset))
            samples.append(sample(timing.CANDIDATE, state, cand + offset))
    return samples


class CaptureStreams:
    def __enter__(self) -> "CaptureStreams":
        self._out, self._err = io.StringIO(), io.StringIO()
        self._saved = (sys.stdout, sys.stderr)
        sys.stdout, sys.stderr = self._out, self._err
        self.text = ""
        return self

    def __exit__(self, *_exc) -> None:
        sys.stdout, sys.stderr = self._saved
        self.text = self._out.getvalue() + self._err.getvalue()


class TimingCase(unittest.TestCase):
    def setUp(self) -> None:
        scratch = ROOT / ".tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temp = Path(tempfile.mkdtemp(dir=scratch))
        self.addCleanup(shutil.rmtree, self.temp, ignore_errors=True)

    def run_compare(self, samples: list[dict], *extra: str) -> tuple[int, str]:
        path = self.temp / "samples.json"
        with path.open("w", encoding="utf-8") as handle:
            json.dump(samples, handle)
        with CaptureStreams() as captured:
            status = timing.main(["--samples", str(path), *extra])
        return status, captured.text


class TestComparableSamples(TimingCase):
    def test_like_samples_report_a_median_per_cache_state(self) -> None:
        status, output = self.run_compare(comparable_set())
        self.assertEqual(status, 0, output)
        self.assertIn("cold", output)
        self.assertIn("warm", output)
        # 660 against 870 is a 24.1% reduction; the point is that cold and warm
        # are reported apart, never pooled into one flattering average.
        self.assertIn("+24.1%", output)
        self.assertIn("+24.6%", output)

    def test_samples_from_two_runner_images_are_refused(self) -> None:
        samples = comparable_set()
        samples[0]["image"] = "ghcr.io/agentsfleet/ci-zig-ubuntu:0.16.0-r3"
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("more than one runner image", output)

    def test_arms_measuring_different_code_are_refused(self) -> None:
        samples = comparable_set()
        for entry in samples:
            if entry["arm"] == timing.CANDIDATE:
                entry["workload_digest"] = "a-branch-that-also-changed-a-test"
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("different code under test", output)

    def test_one_arm_spanning_two_commits_is_refused(self) -> None:
        samples = comparable_set()
        samples[0]["commit"] = "some-other-revision"
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("baseline samples span more than one commit", output)

    def test_unknown_cache_state_is_refused(self) -> None:
        samples = comparable_set()
        samples[0]["cache_state"] = "lukewarm"
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("unknown cache state", output)


class TestSampleDepth(TimingCase):
    def test_too_few_samples_are_refused(self) -> None:
        samples = [entry for entry in comparable_set() if entry["seconds"] % 2 == 0]
        status, output = self.run_compare(samples, "--min-samples", "3")
        self.assertEqual(status, 1)
        self.assertIn("are required", output)

    def test_a_missing_arm_is_refused(self) -> None:
        samples = [entry for entry in comparable_set() if entry["cache_state"] != "warm"]
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("no warm baseline samples", output)


class TestVerdict(TimingCase):
    def test_a_candidate_that_is_slower_fails(self) -> None:
        samples = []
        for state in ("cold", "warm"):
            for offset in (-5.0, 0.0, 5.0):
                samples.append(sample(timing.BASELINE, state, 600.0 + offset))
                samples.append(sample(timing.CANDIDATE, state, 640.0 + offset))
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("not faster in every cache state", output)

    def test_a_candidate_faster_in_only_one_cache_state_fails(self) -> None:
        samples = []
        for state, cand in (("cold", 500.0), ("warm", 640.0)):
            for offset in (-5.0, 0.0, 5.0):
                samples.append(sample(timing.BASELINE, state, 600.0 + offset))
                samples.append(sample(timing.CANDIDATE, state, cand + offset))
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("not faster in every cache state", output)


    def test_a_candidate_with_equal_medians_is_not_an_improvement(self) -> None:
        # A zero-change reading must not be publishable as a saving.
        samples = []
        for state in ("cold", "warm"):
            for offset in (-5.0, 0.0, 5.0):
                samples.append(sample(timing.BASELINE, state, 600.0 + offset))
                samples.append(sample(timing.CANDIDATE, state, 600.0 + offset))
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("not faster in every cache state", output)


class TestMalformedInput(TimingCase):
    def test_a_sample_missing_a_key_names_it(self) -> None:
        samples = comparable_set()
        del samples[0]["seconds"]
        status, output = self.run_compare(samples)
        self.assertEqual(status, 1)
        self.assertIn("is missing seconds", output)

    def test_an_empty_sample_list_is_refused(self) -> None:
        status, output = self.run_compare([])
        self.assertEqual(status, 1)
        self.assertIn("non-empty list", output)


if __name__ == "__main__":
    unittest.main()
