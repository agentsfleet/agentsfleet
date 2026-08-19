#!/usr/bin/env python3
"""Prove the evidence gate refuses everything it is supposed to refuse.

Every assertion here is a negative one dressed as a pair: record a manifest that
would validate, break exactly one thing, and check the failure names that thing.
A validator that accepts a broken manifest is worse than no validator, because
the grade it unblocks reports a number for a build nobody ran.
"""

from __future__ import annotations

import io
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

import verification_evidence as evidence

ROOT = Path(__file__).resolve().parents[1]
UNIT_COMPONENTS = ["agentsfleetd", "runner"]
LIVE_COMPONENTS = ["integration", "lifecycle"]
ALL_COMPONENTS = UNIT_COMPONENTS + LIVE_COMPONENTS
GRAPH = ["agentsfleetd:agentsfleetd-tests", "integration lifecycle", "89"]
SOURCE_PATHS = ["build.zig", "build.zig.zon"]

REPORT_BODY = (
    '<coverage><packages><package><classes>'
    '<class filename="a.zig"><lines>'
    '<line number="1" hits="1"/><line number="2" hits="0"/>'
    "</lines></class></classes></package></packages></coverage>\n"
)
EMPTY_REPORT_BODY = (
    '<coverage><packages><package><classes>'
    '<class filename="a.zig"><lines></lines></class>'
    "</classes></package></packages></coverage>\n"
)


class CaptureStderr:
    """Collect what the validator wrote, so a test can assert on the reason.

    The failures are the product here: a non-zero status only says something is
    wrong, and every one of these tests is about which thing it named.
    """

    def __enter__(self) -> "CaptureStderr":
        self._buffer = io.StringIO()
        self._saved = sys.stderr
        sys.stderr = self._buffer
        self.text = ""
        return self

    def __exit__(self, *_exc) -> None:
        sys.stderr = self._saved
        self.text = self._buffer.getvalue()


class EvidenceCase(unittest.TestCase):
    def setUp(self) -> None:
        scratch = ROOT / ".tmp"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temp = Path(tempfile.mkdtemp(dir=scratch))
        self.addCleanup(shutil.rmtree, self.temp, ignore_errors=True)
        self.coverage = self.temp / "coverage"
        self.unit = self.temp / "unit.json"
        self.integration = self.temp / "integration.json"

    def write_report(self, component: str, body: str = REPORT_BODY) -> Path:
        report = self.coverage / component / f"{component}.hash" / "cobertura.xml"
        report.parent.mkdir(parents=True, exist_ok=True)
        report.write_text(body, encoding="utf-8")
        return report

    def record(
        self,
        producer: str,
        manifest: Path,
        components: list[str],
        *extra: str,
        source_paths: list[str] | None = None,
    ) -> int:
        argv = [
            "record",
            "--repo-root", str(ROOT),
            "--producer", producer,
            "--manifest", str(manifest),
            "--coverage-dir", str(self.coverage),
        ]
        for path in source_paths if source_paths is not None else SOURCE_PATHS:
            argv += ["--source-path", path]
        for part in GRAPH:
            argv += ["--graph", part]
        for component in components:
            argv += ["--component", component]
        return evidence.main(argv + list(extra))

    def validate(
        self,
        expected: list[str] | None = None,
        source_paths: list[str] | None = None,
        graph: list[str] | None = None,
    ) -> int:
        argv = [
            "validate",
            "--repo-root", str(ROOT),
            "--manifest", f"test-coverage-zig:{self.unit}",
            "--manifest", f"test-integration:{self.integration}",
        ]
        for path in source_paths if source_paths is not None else SOURCE_PATHS:
            argv += ["--source-path", path]
        for part in graph if graph is not None else GRAPH:
            argv += ["--graph", part]
        for component in expected if expected is not None else ALL_COMPONENTS:
            argv += ["--expect-component", component]
        return evidence.main(argv)

    def record_both(self) -> None:
        for component in ALL_COMPONENTS:
            self.write_report(component)
        self.assertEqual(self.record("test-coverage-zig", self.unit, UNIT_COMPONENTS), 0)
        self.assertEqual(self.record("test-integration", self.integration, LIVE_COMPONENTS), 0)

    def read(self, manifest: Path) -> dict:
        with manifest.open(encoding="utf-8") as handle:
            return json.load(handle)

    def rewrite(self, manifest: Path, mutate) -> None:
        body = self.read(manifest)
        mutate(body)
        with manifest.open("w", encoding="utf-8") as handle:
            json.dump(body, handle)


class TestMatchedEvidence(EvidenceCase):
    def test_matched_evidence_validates(self) -> None:
        self.record_both()
        self.assertEqual(self.validate(), 0)

    def test_recorded_manifest_names_every_component_and_its_report(self) -> None:
        self.record_both()
        recorded = self.read(self.unit)
        self.assertEqual([c["name"] for c in recorded["components"]], UNIT_COMPONENTS)
        for component in recorded["components"]:
            self.assertGreater(component["measured_lines"], 0)
            self.assertTrue((ROOT / component["report"]).is_file())


class TestProvenance(EvidenceCase):
    def test_mismatched_provenance_field_is_named(self) -> None:
        for field in evidence.PROVENANCE_FIELDS:
            with self.subTest(field=field):
                self.record_both()
                self.rewrite(
                    self.unit,
                    lambda body, f=field: body.__setitem__(f, "from-another-build"),
                )
                with CaptureStderr() as captured:
                    self.assertEqual(self.validate(), 1)
                self.assertIn(field, captured.text)
                self.assertIn("from-another-build", captured.text)


class TestUnusableEvidence(EvidenceCase):
    def test_absent_manifest_names_its_producer(self) -> None:
        self.record_both()
        self.unit.unlink()
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("test-coverage-zig evidence is missing", captured.text)

    def test_failed_outcome_is_refused(self) -> None:
        self.record_both()
        self.rewrite(self.integration, lambda body: body.__setitem__("outcome", "suite-failed"))
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("outcome='suite-failed'", captured.text)

    def test_filtered_run_cannot_support_a_floor(self) -> None:
        for component in ALL_COMPONENTS:
            self.write_report(component)
        self.record("test-coverage-zig", self.unit, UNIT_COMPONENTS)
        self.record("test-integration", self.integration, ["integration"], "--filtered")
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("narrowed run", captured.text)

    def test_empty_component_is_refused(self) -> None:
        self.write_report("agentsfleetd", EMPTY_REPORT_BODY)
        for component in ["runner", *LIVE_COMPONENTS]:
            self.write_report(component)
        self.record("test-coverage-zig", self.unit, UNIT_COMPONENTS)
        self.record("test-integration", self.integration, LIVE_COMPONENTS)
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("component agentsfleetd measured 0 lines", captured.text)

    def test_report_deleted_after_recording_is_refused(self) -> None:
        self.record_both()
        shutil.rmtree(self.coverage / "runner")
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("component runner report is missing", captured.text)

    def test_report_changed_after_recording_is_refused(self) -> None:
        self.record_both()
        self.write_report("integration", REPORT_BODY.replace('hits="0"', 'hits="9"'))
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("changed on disk after it was recorded", captured.text)


class TestUnion(EvidenceCase):
    def test_omitted_component_fails_the_aggregate(self) -> None:
        for component in ALL_COMPONENTS:
            self.write_report(component)
        self.record("test-coverage-zig", self.unit, UNIT_COMPONENTS)
        self.record("test-integration", self.integration, ["integration"])
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("component lifecycle is in the inventory but no lane produced it", captured.text)

    def test_component_produced_by_both_lanes_fails_the_aggregate(self) -> None:
        for component in ALL_COMPONENTS:
            self.write_report(component)
        self.record("test-coverage-zig", self.unit, [*UNIT_COMPONENTS, "integration"])
        self.record("test-integration", self.integration, LIVE_COMPONENTS)
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("component integration was produced more than once", captured.text)

    def test_component_outside_the_inventory_fails_the_aggregate(self) -> None:
        for component in [*ALL_COMPONENTS, "stowaway"]:
            self.write_report(component)
        self.record("test-coverage-zig", self.unit, [*UNIT_COMPONENTS, "stowaway"])
        self.record("test-integration", self.integration, LIVE_COMPONENTS)
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("component stowaway is not in the inventory", captured.text)



class TestRecomputedProvenance(EvidenceCase):
    """Mismatches detected by RECOMPUTING, not by editing the manifest.

    The provenance tests above flip a recorded field, which proves the
    comparison but not the computation — a digest function that returned a
    constant would still pass them. These change the actual input instead.
    """

    def test_a_changed_source_file_refuses_old_evidence(self) -> None:
        source = ROOT / ".tmp" / f"src-{self.temp.name}.zig"
        source.write_text("const answer = 41;\n", encoding="utf-8")
        self.addCleanup(source.unlink)
        paths = [*SOURCE_PATHS, str(source.relative_to(ROOT))]
        self.record_both_over(paths)
        self.assertEqual(self.validate_over(paths), 0)
        source.write_text("const answer = 42;\n", encoding="utf-8")
        with CaptureStderr() as captured:
            self.assertEqual(self.validate_over(paths), 1)
        self.assertIn("source_digest mismatch", captured.text)

    def test_a_changed_graph_refuses_old_evidence(self) -> None:
        self.record_both()
        with CaptureStderr() as captured:
            self.assertEqual(
                self.validate(graph=[*GRAPH, "agentsfleetd=99"]), 1
            )
        self.assertIn("graph_digest mismatch", captured.text)

    def test_graph_whitespace_does_not_change_identity(self) -> None:
        # Make hands these through as strings whose spacing depends on how the
        # variable was written; two spaces must not read as a different graph.
        self.record_both()
        respaced = [part.replace(" ", "  ") for part in GRAPH]
        self.assertEqual(self.validate(graph=respaced), 0)

    def record_both_over(self, paths: list[str]) -> None:
        for component in ALL_COMPONENTS:
            self.write_report(component)
        self.record("test-coverage-zig", self.unit, UNIT_COMPONENTS, source_paths=paths)
        self.record("test-integration", self.integration, LIVE_COMPONENTS, source_paths=paths)

    def validate_over(self, paths: list[str]) -> int:
        return self.validate(source_paths=paths)


class TestMalformedInputs(EvidenceCase):
    def test_unreadable_manifest_json_is_named(self) -> None:
        self.record_both()
        self.unit.write_text("{not json", encoding="utf-8")
        with CaptureStderr() as captured:
            self.assertEqual(self.validate(), 1)
        self.assertIn("not readable JSON", captured.text)

    def test_recording_a_component_with_no_report_fails(self) -> None:
        # Only one of the two components has a report on disk.
        self.write_report("agentsfleetd")
        with CaptureStderr() as captured:
            self.assertEqual(self.record("test-coverage-zig", self.unit, UNIT_COMPONENTS), 1)
        self.assertIn("cobertura.xml", captured.text)
        self.assertFalse(self.unit.exists(), "a failed recording must not leave a manifest")

    def test_a_manifest_argument_without_a_producer_is_refused(self) -> None:
        with CaptureStderr(), self.assertRaises(SystemExit) as caught:
            evidence.main(["validate", "--repo-root", str(ROOT), "--manifest", "just-a-path"])
        self.assertNotEqual(caught.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
