#!/usr/bin/env python3
"""Self-tests for check_zig_coverage.py.

The gate this replaces reported 93.70% while grading 24 of 577 files, because it
trusted `kcov --merge` and never asked what came back. So the tests that matter
here are the ones proving the union refuses to produce a number when a component
drops out, and that a line covered by any one component counts as covered.

Run: python3 -m unittest discover -s scripts -t scripts -p 'check_zig_coverage*_test.py'
"""
import io
import tempfile
import unittest
import xml.etree.ElementTree as ET
from contextlib import redirect_stdout, redirect_stderr
from pathlib import Path

import check_zig_coverage as gate


def write_component(
    root: Path,
    name: str,
    files: dict[str, list[tuple[int, int]]],
    source_root: str | None = None,
) -> None:
    """Lay out one component's kcov output: <root>/<name>/<binary>.<hash>/cobertura.xml."""
    target = root / name / f"{name}-tests.abc123"
    target.mkdir(parents=True, exist_ok=True)
    coverage = ET.Element("coverage")
    if source_root is not None:
        ET.SubElement(ET.SubElement(coverage, "sources"), "source").text = source_root
    classes = ET.SubElement(ET.SubElement(coverage, "packages"), "package")
    container = ET.SubElement(classes, "classes")
    for filename, lines in files.items():
        class_element = ET.SubElement(container, "class", {"filename": filename})
        line_container = ET.SubElement(class_element, "lines")
        for number, hits in lines:
            ET.SubElement(line_container, "line", {"number": str(number), "hits": str(hits)})
    ET.ElementTree(coverage).write(target / "cobertura.xml", encoding="utf-8", xml_declaration=True)


def run_gate(
    root: Path,
    components: list[str],
    min_pct: float,
    merged: Path | None = None,
    required: list[str] | None = None,
) -> tuple[int, str, str]:
    """Invoke main() and capture (exit code, stdout, stderr)."""
    argv = ["--coverage-dir", str(root), "--min-pct", str(min_pct),
            "--repo-root", str(root),
            "--summary-file", str(root / "summary.txt")]
    for name in components:
        argv += ["--component", name]
    for name in required or []:
        argv += ["--require-component", name]
    if merged is not None:
        argv += ["--merged-report", str(merged)]
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        code = gate.main(argv)
    return code, out.getvalue(), err.getvalue()


class UnionSemantics(unittest.TestCase):
    def test_a_line_covered_by_any_component_counts_as_covered(self) -> None:
        """The unit lanes and the integration suite cover disjoint code."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"handler.zig": [(1, 0), (2, 1)]})
            write_component(root, "integration", {"handler.zig": [(1, 5), (2, 0)]})
            code, out, _ = run_gate(root, ["unit", "integration"], 100.0)
            self.assertEqual(code, 0, out)
            self.assertIn("2/2 lines", out)

    def test_disjoint_files_are_summed_not_replaced(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"a.zig": [(1, 1)]})
            write_component(root, "integration", {"b.zig": [(1, 0)]})
            code, out, err = run_gate(root, ["unit", "integration"], 40.0)
            self.assertEqual(code, 0, err)
            self.assertIn("across 2 files", out)


class SourceRootNormalisation(unittest.TestCase):
    """Components root at different depths and must still name the same file once."""

    def test_same_file_from_two_source_roots_counts_once(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # The daemon lane reports relative to src/; the lib lane to src/lib/.
            write_component(root, "daemon", {"lib/common/backoff.zig": [(1, 0), (2, 0)]},
                            source_root=str(root / "src"))
            write_component(root, "lib", {"common/backoff.zig": [(1, 1), (2, 1)]},
                            source_root=str(root / "src" / "lib"))
            code, out, err = run_gate(root, ["daemon", "lib"], 100.0)
            self.assertEqual(code, 0, err)
            self.assertIn("2/2 lines across 1 files", out)

    def test_unnormalised_paths_would_have_halved_the_rate(self) -> None:
        """Without normalisation this reads 2/4 across 2 files — the defect."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "daemon", {"lib/common/backoff.zig": [(1, 0), (2, 0)]},
                            source_root=str(root / "src"))
            write_component(root, "lib", {"common/backoff.zig": [(1, 1), (2, 1)]},
                            source_root=str(root / "src" / "lib"))
            _, out, _ = run_gate(root, ["daemon", "lib"], 0.0)
            self.assertNotIn("across 2 files", out)


class ComponentDropout(unittest.TestCase):
    """The defect this gate exists to catch — a component that stops collecting."""

    def test_required_component_contributing_nothing_fails_the_gate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "lib", {"small.zig": [(1, 1)]})
            write_component(root, "agentsfleetd", {})
            code, _, err = run_gate(root, ["lib", "agentsfleetd"], 50.0,
                                    required=["lib", "agentsfleetd"])
            self.assertEqual(code, 1)
            self.assertIn("agentsfleetd", err)
            self.assertIn("contributed no measured lines", err)

    def test_required_dropout_fails_even_when_survivors_clear_the_floor(self) -> None:
        """100% of a fraction is exactly how 93.70% got reported over 861 lines."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "lib", {"small.zig": [(1, 1), (2, 1)]})
            write_component(root, "agentsfleetd", {})
            code, _, err = run_gate(root, ["lib", "agentsfleetd"], 91.0,
                                    required=["lib", "agentsfleetd"])
            self.assertEqual(code, 1)
            self.assertIn("contributed no measured lines", err)

    def test_unrequired_empty_component_is_graded_over_what_collected(self) -> None:
        """kcov reads two of eight binaries on Linux; refusing leaves no measurement."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "lib", {"small.zig": [(1, 1), (2, 0)]})
            write_component(root, "agentsfleetd", {})
            code, out, err = run_gate(root, ["lib", "agentsfleetd"], 50.0, required=["lib"])
            self.assertEqual(code, 0, err)
            self.assertIn("1/2 lines across 1 files", out)

    def test_every_component_empty_leaves_nothing_to_grade(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "lib", {})
            write_component(root, "agentsfleetd", {})
            code, _, err = run_gate(root, ["lib", "agentsfleetd"], 0.0)
            self.assertEqual(code, 1)
            self.assertIn("nothing to grade", err)

    def test_missing_report_names_the_component(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "lib", {"small.zig": [(1, 1)]})
            (root / "runner").mkdir()
            code, _, err = run_gate(root, ["lib", "runner"], 50.0)
            self.assertEqual(code, 1)
            self.assertIn("no non-empty cobertura.xml", err)


class TestBodiesExcluded(unittest.TestCase):
    def test_test_files_leave_both_numerator_and_denominator(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {
                "product.zig": [(1, 0), (2, 0)],
                "product_test.zig": [(1, 1), (2, 1), (3, 1), (4, 1)],
                "tests.zig": [(1, 1), (2, 1)],
            })
            code, out, err = run_gate(root, ["unit"], 0.0)
            self.assertEqual(code, 0, err)
            self.assertIn("0/2 lines across 1 files", out)


class ThresholdEnforcement(unittest.TestCase):
    def test_below_floor_exits_one_and_reports_both_numbers(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"a.zig": [(1, 1), (2, 0)]})
            code, _, err = run_gate(root, ["unit"], 91.0)
            self.assertEqual(code, 1)
            self.assertIn("50.00% is below threshold 91.00%", err)

    def test_exactly_at_the_floor_passes(self) -> None:
        """Floating point must not turn an exact 50.00 into a failure."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"a.zig": [(1, 1), (2, 0)]})
            code, out, err = run_gate(root, ["unit"], 50.0)
            self.assertEqual(code, 0, err)
            self.assertIn("50.00%", out)

    def test_summary_file_records_measured_and_required(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"a.zig": [(1, 1), (2, 0)]})
            run_gate(root, ["unit"], 91.0)
            written = (root / "summary.txt").read_text(encoding="utf-8")
            self.assertIn("zig_line_coverage_pct=50.00", written)
            self.assertIn("zig_line_coverage_min_pct=91", written)


class SubsetDisclosure(unittest.TestCase):
    """A rate over a subset must never read as a rate over the codebase."""

    def test_scope_line_names_every_component_that_captured_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "runner", {"a.zig": [(1, 1)]})
            write_component(root, "logging", {})
            write_component(root, "deadline", {})
            code, out, err = run_gate(root, ["runner", "logging", "deadline"], 50.0,
                                      required=["runner"])
            self.assertEqual(code, 0, err)
            self.assertIn("measured over 1 of 3 components", out)
            self.assertIn("deadline, logging", out)

    def test_a_full_capture_says_so_rather_than_warning(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "runner", {"a.zig": [(1, 1)]})
            code, out, _ = run_gate(root, ["runner"], 50.0, required=["runner"])
            self.assertIn("every component collected", out)
            self.assertNotIn("⚠", out)

    def test_a_breach_reports_the_subset_alongside_the_shortfall(self) -> None:
        """The floor failing is when reading the number as whole-codebase misleads most."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "runner", {"a.zig": [(1, 1), (2, 0)]})
            write_component(root, "logging", {})
            code, _, err = run_gate(root, ["runner", "logging"], 91.0, required=["runner"])
            self.assertEqual(code, 1)
            self.assertIn("is below threshold", err)
            self.assertIn("measured over 1 of 2 components", err)

    def test_summary_file_publishes_the_denominator_and_the_component_counts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "runner", {"a.zig": [(1, 1), (2, 0)]})
            write_component(root, "logging", {})
            run_gate(root, ["runner", "logging"], 0.0, required=["runner"])
            written = (root / "summary.txt").read_text(encoding="utf-8")
            self.assertIn("zig_measured_files=1", written)
            self.assertIn("zig_measured_lines=2", written)
            self.assertIn("zig_components_measured=1", written)
            self.assertIn("zig_components_total=2", written)
            self.assertIn("zig_components_empty=logging", written)


class MergedReport(unittest.TestCase):
    def test_published_report_agrees_with_the_enforced_number(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            merged = root / "merged"
            write_component(root, "unit", {"a.zig": [(1, 1), (2, 0)]})
            run_gate(root, ["unit"], 0.0, merged=merged)
            published = ET.parse(merged / "cobertura.xml").getroot()
            self.assertEqual(published.get("lines-covered"), "1")
            self.assertEqual(published.get("lines-valid"), "2")
            self.assertIn("50.00%", (merged / "summary.txt").read_text(encoding="utf-8"))

    def test_stale_contents_are_cleared_before_publishing(self) -> None:
        """kcov's old merge output must not ship beside ours disagreeing."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            merged = root / "merged"
            (merged / "kcov-merged").mkdir(parents=True)
            (merged / "kcov-merged" / "cobertura.xml").write_text("<coverage/>", encoding="utf-8")
            write_component(root, "unit", {"a.zig": [(1, 1)]})
            run_gate(root, ["unit"], 0.0, merged=merged)
            self.assertFalse((merged / "kcov-merged").exists())
            self.assertTrue((merged / "cobertura.xml").exists())


if __name__ == "__main__":
    unittest.main()
