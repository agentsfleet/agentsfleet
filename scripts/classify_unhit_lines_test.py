#!/usr/bin/env python3
"""Self-test for the unhit-line classifier.

The classifier is the sweep's grading instrument: rubric rows R1-R4 read its
counts, so a rule that silently misfiles a line moves a grade without moving
any code. These cases pin the decisions that are easy to get wrong — the
continuation lines of a multi-line call, the body of an `errdefer` block, and
the precedence between classes that can both match one line.
"""

from __future__ import annotations

import tempfile
import textwrap
import unittest
from pathlib import Path

import classify_unhit_lines as classifier


def write_report(root: Path, path: str, unhit: list[int], hit: list[int]) -> Path:
    """A minimal Cobertura report naming one file's hit and unhit lines."""
    lines = "".join(f'<line number="{n}" hits="0" />' for n in unhit)
    lines += "".join(f'<line number="{n}" hits="3" />' for n in hit)
    report = root / "cobertura.xml"
    report.write_text(
        "<coverage><packages><package><classes>"
        f'<class name="{path}" filename="{path}"><lines>{lines}</lines></class>'
        "</classes></package></packages></coverage>"
    )
    return report


class ScanCase(unittest.TestCase):
    """The bracket scanner the continuation and block logic both rest on."""

    def test_counts_real_syntax_only(self) -> None:
        self.assertEqual(classifier.scan_deltas("fn f() void {"), (1, 0))
        self.assertEqual(classifier.scan_deltas("call(a, b);"), (0, 0))
        self.assertEqual(classifier.scan_deltas("call(a,"), (0, 1))

    def test_ignores_brackets_in_text(self) -> None:
        # A brace inside a string or a comment is text. Counting it drifts the
        # depth, and every line after a drift joins the wrong class.
        self.assertEqual(classifier.scan_deltas('log.err("{{{");'), (0, 0))
        self.assertEqual(classifier.scan_deltas("const x = 1; // {{{"), (0, 0))
        self.assertEqual(classifier.scan_deltas("const c = '{';"), (0, 0))


class HeadCase(unittest.TestCase):
    """Continuation lines belong to the statement that opened them."""

    def test_multiline_call_attributes_to_its_first_line(self) -> None:
        source = [
            "log.err(",
            '    "boom",',
            "    .{ .err = 1 },",
            ");",
            "const after = 2;",
        ]
        self.assertEqual(classifier.statement_heads(source), [0, 0, 0, 0, 4])

    def test_block_braces_do_not_open_a_continuation(self) -> None:
        # Only `(` and `[` continue a statement. If `{` did too, every line in
        # a function body would attribute to the `fn` line and the whole file
        # would classify as one statement.
        source = ["fn f() void {", "    const a = 1;", "}"]
        self.assertEqual(classifier.statement_heads(source), [0, 1, 2])


class ErrdeferBlockCase(unittest.TestCase):
    """The block form claims its body, which carries no keyword of its own."""

    def test_body_and_braces_are_inside(self) -> None:
        source = [
            "errdefer {",
            "    alloc.free(a);",
            "}",
            "const after = 1;",
        ]
        self.assertEqual(classifier.errdefer_lines(source), {0, 1, 2})

    def test_nested_braces_do_not_close_it_early(self) -> None:
        source = [
            "errdefer {",
            "    if (x) {",
            "        alloc.free(a);",
            "    }",
            "}",
            "const after = 1;",
        ]
        self.assertEqual(classifier.errdefer_lines(source), {0, 1, 2, 3, 4})


class ClassifyLineCase(unittest.TestCase):
    """Precedence between classes that can both match one line."""

    def test_each_class_matches_its_own_shape(self) -> None:
        cases = {
            "errdefer alloc.free(id);": classifier.CLASS_ERRDEFER,
            "return error.Invalid;": classifier.CLASS_ERROR_RETURN,
            "hx.fail(ec.ERR_INVALID_REQUEST, MSG);": classifier.CLASS_FAILURE_RESPONSE,
            "common.internalDbError(hx.res, hx.req_id);": classifier.CLASS_FAILURE_RESPONSE,
            'log.warn("boom", .{});': classifier.CLASS_FAILURE_LOG,
            "}": classifier.CLASS_BRACE,
            "const total = a + b;": classifier.CLASS_OTHER,
        }
        for text, expected in cases.items():
            with self.subTest(text=text):
                self.assertEqual(classifier.classify_line(text, False), expected)

    def test_cleanup_outranks_what_it_contains(self) -> None:
        # A rung that logs is still cleanup: an induced allocation failure is
        # what reaches it, and that is what decides which test must exist.
        self.assertEqual(
            classifier.classify_line('log.warn("undo", .{});', True),
            classifier.CLASS_ERRDEFER,
        )

    def test_healthy_log_levels_are_not_failure_logs(self) -> None:
        # `debug` and `info` fire on success paths too; counting them would
        # inflate the class with lines no failure reaches.
        for text in ('log.debug("x", .{});', 'log.info("x", .{});'):
            with self.subTest(text=text):
                self.assertEqual(
                    classifier.classify_line(text, False), classifier.CLASS_OTHER
                )


class ClassifyFileCase(unittest.TestCase):
    """End to end over a real file and report."""

    def setUp(self) -> None:
        raw = tempfile.TemporaryDirectory()
        self.addCleanup(raw.cleanup)
        self.root = Path(raw.name)

    def write_source(self, body: str) -> str:
        path = "src/agentsfleetd/sample.zig"
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(textwrap.dedent(body).lstrip("\n"))
        return path

    def test_only_unhit_lines_are_reported(self) -> None:
        path = self.write_source(
            """
            const a = 1;
            errdefer alloc.free(a);
            const b = 2;
            """
        )
        report = write_report(self.root, path, unhit=[2], hit=[1, 3])
        found = classifier.classify(report, self.root)
        self.assertEqual([(f.number, f.kind) for f in found], [(2, "errdefer")])

    def test_inline_test_bodies_are_dropped(self) -> None:
        # Invariant 6: a class must not be emptiable by writing test code
        # inside a product file, so the same exclusion the coverage gate
        # applies is applied here.
        path = self.write_source(
            """
            const a = 1;
            test "sample" {
                const unreached = 2;
            }
            """
        )
        report = write_report(self.root, path, unhit=[3], hit=[1])
        self.assertEqual(classifier.classify(report, self.root), [])

    def test_missing_source_is_skipped_not_fatal(self) -> None:
        report = write_report(self.root, "src/gone.zig", unhit=[1], hit=[])
        self.assertEqual(classifier.classify(report, self.root), [])


class RenderCase(unittest.TestCase):
    """The count surface the rubric grades from."""

    def setUp(self) -> None:
        self.found = [
            classifier.UnhitLine("a.zig", 1, "errdefer x;", classifier.CLASS_ERRDEFER),
            classifier.UnhitLine("a.zig", 2, "}", classifier.CLASS_BRACE),
        ]

    def test_count_is_a_bare_number(self) -> None:
        out = classifier.render(self.found, (classifier.CLASS_ERRDEFER,), True)
        self.assertEqual(out, "1")

    def test_count_spans_every_named_class(self) -> None:
        wanted = (classifier.CLASS_ERRDEFER, classifier.CLASS_BRACE)
        self.assertEqual(classifier.render(self.found, wanted, True), "2")

    def test_listing_names_file_and_line(self) -> None:
        out = classifier.render(self.found, (classifier.CLASS_ERRDEFER,), False)
        self.assertIn("a.zig:1", out)
        self.assertIn("total 1", out)


class ArgumentCase(unittest.TestCase):
    """Argument handling, including the ways a caller gets it wrong."""

    def test_default_is_every_class(self) -> None:
        self.assertEqual(classifier.parse_classes(None), classifier.CLASS_NAMES)

    def test_comma_list_preserves_order(self) -> None:
        self.assertEqual(
            classifier.parse_classes("other,brace"),
            (classifier.CLASS_OTHER, classifier.CLASS_BRACE),
        )

    def test_unknown_class_is_a_usage_error(self) -> None:
        with self.assertRaises(classifier.UsageError):
            classifier.parse_classes("errdefer,typo")

    def test_missing_report_is_a_usage_error(self) -> None:
        with self.assertRaises(classifier.UsageError):
            classifier.classify(Path("/nonexistent/cobertura.xml"), Path("."))

    def test_main_returns_two_on_usage_error(self) -> None:
        self.assertEqual(classifier.main(["--class", "typo"]), 2)


if __name__ == "__main__":
    unittest.main()
