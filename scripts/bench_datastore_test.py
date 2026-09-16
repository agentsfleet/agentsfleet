#!/usr/bin/env python3
"""What the evidence grader refuses, which is Dimension 6.3.

`test_evidence_refuses_unsafe_or_incomplete_runs` is this file. Each case
builds a directory of result files, grades it, and asserts on the violation the
grader names — a grader that failed for the wrong reason would still be red,
and a red-for-the-wrong-reason gate is how a real defect hides behind a
cosmetic one.

The passing case is here too and matters as much: a gate nothing can satisfy is
one people learn to route around.
"""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import bench_datastore  # noqa: E402


def report(**overrides):
    """A result file that passes every universal check."""
    base = {
        "lane": "lease",
        "profile": "rig",
        "created": True,
        "provenance": {
            "revision": "3643941ca",
            "datastore_image": "docker.dragonflydb.io/dragonflydb/dragonfly:v1.40.2",
            "owned": True,
        },
        "parameters": {},
        "measurements": {
            "error_rate": 0.0,
            "silent_drops": 0.0,
            "proofs": 4.0,
            "failures": 0.0,
            "missing_admissions": 0.0,
            "duplicate_settlements": 0.0,
            "duplicate_debits": 0.0,
        },
        "datastores": {},
        "fixture": {"run_prefix": "bench-1", "created": 8, "swept": 8},
    }
    base.update(overrides)
    return base


class GraderCase(unittest.TestCase):
    """A scratch results directory per case, populated per test."""

    def setUp(self):
        self._scratch = tempfile.TemporaryDirectory()
        self.results = Path(self._scratch.name)
        self.addCleanup(self._scratch.cleanup)

    def archive(self, check, **per_file):
        """Write every file `check` requires, applying overrides by stem."""
        for stem in bench_datastore.MODES[check]["files"]:
            body = per_file.get(stem, report())
            if body is not None:
                (self.results / f"{stem}.rig.json").write_text(
                    json.dumps(body), encoding="utf-8"
                )

    def grade(self, check):
        return bench_datastore.grade(check, "rig", results=self.results)


class CompleteEvidencePasses(GraderCase):
    def test_every_mode_passes_on_complete_owned_swept_evidence(self):
        # One assertion per mode rather than one for the set: a grader that
        # only ever passed `cluster` would look healthy while four rows could
        # not be satisfied at all.
        for check in sorted(bench_datastore.MODES):
            with self.subTest(check=check):
                self.setUp()
                self.archive(check)
                self.assertEqual(self.grade(check), [])


class IncompleteEvidenceFails(GraderCase):
    def test_a_missing_file_is_not_a_row_that_passed(self):
        self.archive("coordination", lease=None)
        violations = self.grade("coordination")
        self.assertEqual(len(violations), 1, violations)
        self.assertIn("lease.rig.json", violations[0])
        self.assertIn("missing", violations[0])

    def test_a_run_with_no_provenance_block_is_refused(self):
        body = report()
        del body["provenance"]
        self.archive("coordination", lease=body)
        self.assertTrue(
            any("no provenance block" in v for v in self.grade("coordination"))
        )

    def test_an_empty_revision_or_image_is_refused_by_name(self):
        for field in ("revision", "datastore_image"):
            with self.subTest(field=field):
                self.setUp()
                body = report()
                body["provenance"][field] = "   "
                self.archive("coordination", lease=body)
                self.assertTrue(
                    any(f"provenance.{field} is empty" in v for v in self.grade("coordination"))
                )

    def test_a_measurement_the_row_needs_cannot_be_absent(self):
        body = report()
        del body["measurements"]["missing_admissions"]
        self.archive("durability", durability=body)
        self.assertTrue(
            any("no missing_admissions measured" in v for v in self.grade("durability"))
        )

    def test_an_aborted_run_is_not_evidence(self):
        body = report(abort={"reason": "error rate exceeded", "threshold": 0.5})
        self.archive("coordination", steer=body)
        self.assertTrue(any("aborted early" in v for v in self.grade("coordination")))


class UnsafeEvidenceFails(GraderCase):
    def test_a_shared_target_fails_every_mode_that_names_the_file(self):
        body = report()
        body["provenance"]["owned"] = False
        self.archive("coordination", lease=body)
        violations = self.grade("coordination")
        self.assertTrue(any("did not own" in v for v in violations), violations)

    def test_ownership_must_be_true_and_not_merely_truthy(self):
        # A file hand-written with "owned": "yes" claims ownership to a reader
        # and must not to the grader.
        for claimed in ("yes", "true", 1, "owned"):
            with self.subTest(claimed=claimed):
                self.setUp()
                body = report()
                body["provenance"]["owned"] = claimed
                self.archive("coordination", lease=body)
                self.assertTrue(
                    any("did not own" in v for v in self.grade("coordination"))
                )

    def test_a_leaked_fixture_fails_even_though_the_numbers_look_fine(self):
        body = report(fixture={"run_prefix": "bench-1", "created": 600, "swept": 597})
        self.archive("coordination", lease=body)
        violations = self.grade("coordination")
        self.assertTrue(any("leaked 3 of 600" in v for v in violations), violations)

    def test_a_budget_break_is_reported_with_its_direction(self):
        body = report()
        body["measurements"]["error_rate"] = 0.02
        self.archive("coordination", lease=body)
        violations = self.grade("coordination")
        self.assertTrue(
            any("error_rate=0.02 breaks at_most 0.0" in v for v in violations), violations
        )

    def test_too_few_prototype_proofs_is_a_break_in_the_other_direction(self):
        body = report()
        body["measurements"]["proofs"] = 3.0
        self.archive("prototype", prototype=body)
        violations = self.grade("prototype")
        self.assertTrue(
            any("proofs=3.0 breaks at_least 4.0" in v for v in violations), violations
        )


class ModeVocabulary(GraderCase):
    def test_an_unknown_check_names_the_ones_that_exist(self):
        violations = bench_datastore.grade("durabilty", "rig", results=self.results)
        self.assertEqual(len(violations), 1)
        self.assertIn("unknown CHECK", violations[0])
        self.assertIn("durability", violations[0])

    def test_the_cluster_row_names_every_other_rows_evidence(self):
        # R5 is the coverage row. If a mode's file is added and `cluster` is not
        # extended, a suite loses its cluster evidence requirement silently.
        named = set(bench_datastore.MODES["cluster"]["files"])
        for check, mode in bench_datastore.MODES.items():
            if check == "cluster":
                continue
            self.assertLessEqual(
                set(mode["files"]),
                named,
                f"{check} names evidence the cluster row does not require",
            )

    def test_an_unreadable_file_is_refused_rather_than_read_as_empty(self):
        (self.results / "lease.rig.json").write_text("{ not json", encoding="utf-8")
        (self.results / "steer.rig.json").write_text(
            json.dumps(report()), encoding="utf-8"
        )
        violations = self.grade("coordination")
        self.assertTrue(any("unreadable as a result file" in v for v in violations))


if __name__ == "__main__":
    unittest.main()
