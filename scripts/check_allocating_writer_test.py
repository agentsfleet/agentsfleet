#!/usr/bin/env python3
"""Fixture proof for lint-zig.py's allocating-writer check.

`std.Io.Writer.Allocating.fromArrayList` takes OWNERSHIP of the source list
and resets it to empty, so a pre-existing `defer list.deinit()` frees nothing
— the accumulated bytes leak on every path that skips the writer's own
deinit. The checker demands a `defer <name>.deinit()` (or errdefer) pairing
on every binding; these fixtures prove it bites the leak shapes and passes
the clean and suppressed ones.
"""

import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

_spec = importlib.util.spec_from_file_location("lint_zig", ROOT / "lint-zig.py")
lint_zig = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lint_zig)

check = lint_zig.check_allocating_writer_text

LEAK_FROM_ARRAY_LIST = (
    "fn leak() !void {\n"
    "    var body: std.ArrayList(u8) = .empty;\n"
    "    defer body.deinit(alloc);\n"
    "    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &body);\n"
    "}\n"
)

LEAK_QUALIFIED_INIT = (
    "fn leakQualified() !void {\n"
    "    var aw = std.Io.Writer.Allocating.init(alloc);\n"
    "}\n"
)

CLEAN_DEFER = (
    "fn clean() !void {\n"
    "    var aw: std.Io.Writer.Allocating = .init(alloc);\n"
    "    defer aw.deinit();\n"
    "}\n"
)

CLEAN_ERRDEFER = (
    "fn cleanErrdefer() ![]u8 {\n"
    "    var aw: std.Io.Writer.Allocating = .init(alloc);\n"
    "    errdefer aw.deinit();\n"
    "    return aw.toOwnedSlice();\n"
    "}\n"
)

SUPPRESSED = (
    "fn suppressed() !void {\n"
    "    // check-allocating-writer: ok — ownership transferred to caller\n"
    "    var aw: std.Io.Writer.Allocating = .init(alloc);\n"
    "}\n"
)

COMMENTED_OUT = (
    "fn commented() !void {\n"
    "    // var aw: std.Io.Writer.Allocating = .init(alloc);\n"
    "}\n"
)

# A URL literal shares the line with the deinit: the comment-strip must not
# treat "://" as a line comment and delete the pairing (review find).
URL_ON_DEINIT_LINE = (
    "fn urlLine() !void {\n"
    "    var aw: std.Io.Writer.Allocating = .init(alloc);\n"
    '    const u = "https://example.com"; defer aw.deinit();\n'
    "    _ = u;\n"
    "}\n"
)


class AllocatingWriterCheckTest(unittest.TestCase):
    def test_flags_from_array_list_with_noop_list_defer(self):
        self.assertEqual(1, len(check(LEAK_FROM_ARRAY_LIST, "fixture")))

    def test_flags_qualified_init_without_deinit(self):
        self.assertEqual(1, len(check(LEAK_QUALIFIED_INIT, "fixture")))

    def test_passes_defer_pairing(self):
        self.assertEqual(0, len(check(CLEAN_DEFER, "fixture")))

    def test_passes_errdefer_pairing(self):
        self.assertEqual(0, len(check(CLEAN_ERRDEFER, "fixture")))

    def test_honors_suppression_comment(self):
        self.assertEqual(0, len(check(SUPPRESSED, "fixture")))

    def test_ignores_commented_out_bindings(self):
        self.assertEqual(0, len(check(COMMENTED_OUT, "fixture")))

    def test_url_literal_does_not_eat_the_deinit(self):
        self.assertEqual(0, len(check(URL_ON_DEINIT_LINE, "fixture")))

    def test_live_tree_is_clean(self):
        violations = []
        for path in sorted((ROOT / "src").rglob("*.zig")):
            violations.extend(lint_zig.check_allocating_writer(path))
        self.assertEqual([], violations)


if __name__ == "__main__":
    unittest.main()
