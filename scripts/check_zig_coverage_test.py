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
    folder_floors: list[str] | None = None,
    folder_targets: list[str] | None = None,
    target_pct: float | None = None,
    required_roots: list[str] | None = None,
    min_files: int | None = None,
    min_lines: int | None = None,
) -> tuple[int, str, str]:
    """Invoke main() and capture (exit code, stdout, stderr)."""
    argv = ["--coverage-dir", str(root), "--min-pct", str(min_pct),
            "--repo-root", str(root),
            "--summary-file", str(root / "summary.txt")]
    for name in components:
        argv += ["--component", name]
    for name in required or []:
        argv += ["--require-component", name]
    for pair in folder_floors or []:
        argv += ["--folder-floor", pair]
    for pair in folder_targets or []:
        argv += ["--folder-target", pair]
    for name in required_roots or []:
        argv += ["--require-root", name]
    if target_pct is not None:
        argv += ["--target-pct", str(target_pct)]
    if min_files is not None:
        argv += ["--min-files", str(min_files)]
    if min_lines is not None:
        argv += ["--min-lines", str(min_lines)]
    if merged is not None:
        argv += ["--merged-report", str(merged)]
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        code = gate.main(argv)
    return code, out.getvalue(), err.getvalue()


def read_summary(root: Path) -> dict[str, str]:
    """The key/value surface the Continuous Integration summary step reads."""
    text = (root / "summary.txt").read_text(encoding="utf-8")
    pairs = (line.partition("=") for line in text.splitlines() if line)
    return {key: value for key, _separator, value in pairs}


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


class TestSupportExcluded(unittest.TestCase):
    """Harness is not shipped code. A gate satisfiable by writing more harness
    measures the wrong thing, and at a 95% target it is wider than the margin."""

    def test_support_naming_forms_excluded(self) -> None:
        support = {
            f"src/agentsfleetd/http/{name}": [(1, 0)]
            for name in gate.floors.TEST_SUPPORT_NAMES
        }
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"src/agentsfleetd/a.zig": [(1, 1)], **support})
            code, out, err = run_gate(root, ["unit"], 100.0)
            self.assertEqual(code, 0, err)
            self.assertIn("1/1 lines", out)
            self.assertIn("across 1 files", out)

    def test_fixture_suffix_families_excluded(self) -> None:
        """Suffix families, so the next fixture module added does not rejoin."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {
                "src/agentsfleetd/a.zig": [(1, 1)],
                "src/agentsfleetd/webhook_test_fixtures.zig": [(1, 0)],
                "src/agentsfleetd/http_test_harness.zig": [(1, 0)],
                "src/agentsfleetd/db_test_support.zig": [(1, 0)],
            })
            code, out, err = run_gate(root, ["unit"], 100.0)
            self.assertEqual(code, 0, err)
            self.assertIn("across 1 files", out)

    def test_product_helpers_retained(self) -> None:
        """Product files whose names read test-adjacent stay in the denominator."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {
                "src/agentsfleetd/fleet_runtime/config_helpers.zig": [(1, 1)],
                "src/agentsfleetd/http/handlers/auth/session_helpers.zig": [(1, 1)],
                "src/agentsfleetd/http/handlers/memory/helpers.zig": [(1, 1)],
            })
            code, out, err = run_gate(root, ["unit"], 100.0)
            self.assertEqual(code, 0, err)
            self.assertIn("across 3 files", out)

    def test_excluded_form_absent_from_union(self) -> None:
        """Two components both reporting the same harness file contribute zero."""
        harness = {"src/agentsfleetd/http/test_harness.zig": [(1, 1), (2, 1)]}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"src/agentsfleetd/a.zig": [(1, 0)], **harness})
            write_component(root, "integration", harness)
            code, out, _ = run_gate(root, ["unit", "integration"], 0.0)
            self.assertEqual(code, 0)
            self.assertIn("0/1 lines", out)


class InlineTestBodiesExcluded(unittest.TestCase):
    """A `test` block inside a product file is a test body, not shipped code.

    kcov's --exclude-pattern drops `*_test.zig` FILES and nothing dropped these.
    Because a test body is ~100% covered by construction, counting them lifted
    every rate — the gate was partly satisfiable by writing more tests, which is
    the exact failure the file-level exclusion exists to prevent.
    """

    def _write_source(self, root: Path, body: str) -> None:
        target = root / "src" / "agentsfleetd" / "widget.zig"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body, encoding="utf-8")

    def test_lines_inside_a_test_block_leave_the_denominator(self) -> None:
        source = (
            "const std = @import(\"std\");\n"  # 1
            "pub fn add(a: u8, b: u8) u8 {\n"  # 2
            "    return a + b;\n"  # 3
            "}\n"  # 4
            "\n"  # 5
            "test \"add sums\" {\n"  # 6
            "    try std.testing.expectEqual(3, add(1, 2));\n"  # 7
            "    try std.testing.expectEqual(0, add(0, 0));\n"  # 8
            "}\n"  # 9
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_source(root, source)
            write_component(root, "unit", {
                "src/agentsfleetd/widget.zig": [(2, 1), (3, 1), (6, 1), (7, 1), (8, 1)],
            })
            code, out, err = run_gate(root, ["unit"], 100.0)
            self.assertEqual(code, 0, err)
            # Only lines 2 and 3 are product; 6-8 are the test body.
            self.assertIn("2/2 lines", out)

    def test_an_uncovered_product_line_still_counts_beside_a_test_block(self) -> None:
        """Excluding test bodies must not also excuse the code under test."""
        source = (
            "pub fn risky(x: u8) u8 {\n"  # 1
            "    if (x == 0) return 1;\n"  # 2
            "    return x;\n"  # 3
            "}\n"  # 4
            "test \"only the easy arm\" {\n"  # 5
            "    _ = risky(0);\n"  # 6
            "}\n"  # 7
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_source(root, source)
            write_component(root, "unit", {
                "src/agentsfleetd/widget.zig": [(1, 1), (2, 1), (3, 0), (5, 1), (6, 1)],
            })
            code, out, err = run_gate(root, ["unit"], 0.0)
            self.assertEqual(code, 0, err)
            self.assertIn("2/3 lines", out)

    def test_a_bare_test_block_is_excluded_too(self) -> None:
        """`test { }` — the import-chaining form every source file carries."""
        source = (
            "pub const value = 1;\n"  # 1
            "test {\n"  # 2
            "    _ = @import(\"widget_test.zig\");\n"  # 3
            "}\n"  # 4
        )
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._write_source(root, source)
            write_component(root, "unit", {
                "src/agentsfleetd/widget.zig": [(1, 1), (2, 1), (3, 1)],
            })
            code, out, err = run_gate(root, ["unit"], 100.0)
            self.assertEqual(code, 0, err)
            self.assertIn("1/1 lines", out)

    def test_a_source_file_that_cannot_be_read_keeps_every_line(self) -> None:
        """No source on disk means no exclusion — never silently drop a line."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"src/agentsfleetd/absent.zig": [(1, 1), (2, 0)]})
            code, out, err = run_gate(root, ["unit"], 0.0)
            self.assertEqual(code, 0, err)
            self.assertIn("1/2 lines", out)


class DenominatorAssertions(unittest.TestCase):
    """No rate is graded before its denominator. A percentage over a report that
    lost most of the tree is not a measurement, however high it reads."""

    def test_collapsed_report_fails_before_any_rate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"src/agentsfleetd/a.zig": [(1, 1)]})
            code, _, err = run_gate(root, ["unit"], 0.0, min_files=300, min_lines=18000)
            self.assertEqual(code, 1)
            self.assertIn("union measured 1 files / 1 lines", err)
            self.assertIn("300 files / 18000 lines", err)

    def test_a_healthy_report_clears_the_collapse_alarm(self) -> None:
        """The alarm is set near half the measured figures — it must not graze."""
        files = {f"src/agentsfleetd/f{index}.zig": [(1, 1)] for index in range(400)}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", files)
            code, _, err = run_gate(root, ["unit"], 0.0, min_files=300, min_lines=300)
            self.assertEqual(code, 0, err)

    def test_absent_product_root_fails_despite_high_rate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_component(root, "unit", {"src/lib/a.zig": [(1, 1)]})
            code, _, err = run_gate(
                root, ["unit"], 0.0, required_roots=["agentsfleetd", "runner", "lib"]
            )
            self.assertEqual(code, 1)
            self.assertIn("agentsfleetd", err)
            self.assertIn("runner", err)
            self.assertNotIn("no measured line for required product root(s): lib", err)


class PerFolderFloors(unittest.TestCase):
    """One merged figure cannot bind three trees moving independently."""

    def _tree(self, root: Path) -> None:
        # agentsfleetd 1/2 = 50%; runner 2/2 = 100%; union 3/4 = 75%.
        write_component(root, "unit", {
            "src/agentsfleetd/a.zig": [(1, 1), (2, 0)],
            "src/runner/b.zig": [(1, 1), (2, 1)],
        })

    def test_folder_breach_names_folder_and_floor(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._tree(root)
            code, _, err = run_gate(root, ["unit"], 0.0, folder_floors=["agentsfleetd=90"])
            self.assertEqual(code, 1)
            self.assertIn("agentsfleetd line coverage 50.00% is below threshold 90.00%", err)

    def test_a_healthy_folder_is_not_blamed_for_its_sibling(self) -> None:
        """The whole point: the runner must not go red because the daemon fell."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._tree(root)
            code, _, err = run_gate(
                root, ["unit"], 0.0, folder_floors=["agentsfleetd=90", "runner=95"]
            )
            self.assertEqual(code, 1)
            self.assertIn("agentsfleetd line coverage", err)
            self.assertNotIn("runner line coverage", err)

    def test_enforced_floors_clear_measured_values(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._tree(root)
            code, out, err = run_gate(
                root, ["unit"], 70.0, folder_floors=["agentsfleetd=50", "runner=100"]
            )
            self.assertEqual(code, 0, err)
            self.assertIn("agentsfleetd", out)
            self.assertIn("runner", out)

    def test_gap_to_target_published_and_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._tree(root)
            code, _, err = run_gate(
                root, ["unit"], 0.0, target_pct=95.0,
                folder_targets=["agentsfleetd=95", "runner=95"],
            )
            self.assertEqual(code, 0, err)
            keys = read_summary(root)
            self.assertEqual(keys["zig_folder_pct_agentsfleetd"], "50.00")
            self.assertEqual(keys["zig_folder_target_pct_agentsfleetd"], "95")
            self.assertEqual(keys["zig_folder_gap_pts_agentsfleetd"], "45.00")
            # Target met — the gap floors at zero rather than going negative.
            self.assertEqual(keys["zig_folder_gap_pts_runner"], "0.00")

    def test_floor_above_its_own_target_is_a_usage_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._tree(root)
            code, _, err = run_gate(
                root, ["unit"], 0.0,
                folder_floors=["agentsfleetd=96"], folder_targets=["agentsfleetd=95"],
            )
            self.assertEqual(code, 1)
            self.assertIn("above its own target", err)

    def test_floor_for_an_unmeasured_scope_is_not_silently_ignored(self) -> None:
        """A folder renamed in the tree but not in make/test.mk must say so."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._tree(root)
            code, _, err = run_gate(root, ["unit"], 0.0, folder_floors=["daemon=90"])
            self.assertEqual(code, 1)
            self.assertIn("no component measured", err)
            self.assertIn("daemon", err)

    def test_every_published_rate_carries_its_floor_and_target(self) -> None:
        """There is no code path emitting a percentage on its own."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._tree(root)
            run_gate(root, ["unit"], 0.0, target_pct=95.0,
                     folder_floors=["agentsfleetd=10"], folder_targets=["agentsfleetd=95"])
            keys = read_summary(root)
            for folder in ("agentsfleetd", "runner"):
                for prefix in ("pct", "min_pct", "target_pct", "gap_pts"):
                    self.assertIn(f"zig_folder_{prefix}_{folder}", keys)
            self.assertIn("zig_line_coverage_target_pct", keys)
            self.assertIn("zig_line_coverage_gap_pts", keys)


if __name__ == "__main__":
    unittest.main()
