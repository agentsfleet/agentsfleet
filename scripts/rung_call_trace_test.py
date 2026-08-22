#!/usr/bin/env python3
"""Self-test for the transitive (file, function) walk.

The one-hop answer inherited its callers' FILE class, and that is how six of
eight leak-capable files in `state/**` read as leak-capable while every path out
of them ended in a handler. These cases pin the walk that replaced it — including
the two that decide nothing on their own: a cycle, and a lost call trail.
"""

from __future__ import annotations

import tempfile
import textwrap
import unittest
from pathlib import Path

import rung_call_edges as edges
import rung_call_trace as trace


def write(root: Path, rel: str, body: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(body), encoding="utf-8")


class RungCallTraceTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.src = Path(self._tmp.name) / "src"
        self.src.mkdir()
        self.addCleanup(self._tmp.cleanup)

    def tree(self, file_class: dict[str, str] | None = None) -> trace.Tree:
        files = [f for f in edges.zig_sources(str(self.src)) if not edges.is_test_path(f)]
        return trace.Tree(
            str(self.src),
            files,
            str(self.src / "agentsfleetd/http/handlers"),
            [str(self.src / "agentsfleetd/cron/FireService.zig")],
            [str(self.src / "agentsfleetd/cmd/boot.zig")],
            {str(self.src / k): v for k, v in (file_class or {}).items()},
        )

    def resolve(self, rel: str, fn: str, file_class=None) -> trace.Verdict:
        return self.tree(file_class).resolve(trace.Site(str(self.src / rel), fn))

    def test_a_mixed_file_answers_per_function(self) -> None:
        """The bug the walk exists for, at its smallest.

        One file, two functions, one worker and one handler. The FILE is
        long-lived either way; only the per-function answer separates the rung
        worth proving from the one that cannot leak.
        """
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const vault = @import("../state/vault.zig");
            fn tick() void { vault.loadJson(); }
        """)
        write(self.src, "agentsfleetd/http/handlers/thing.zig", """
            const vault = @import("../../state/vault.zig");
            fn handle() void { vault.loadMetadata(); }
        """)
        write(self.src, "agentsfleetd/state/vault.zig", """
            pub fn loadJson() void { errdefer cleanup(); }
            pub fn loadMetadata() void { errdefer cleanup(); }
        """)
        self.assertEqual(self.resolve("agentsfleetd/state/vault.zig", "loadJson").cls,
                         trace.CLASS_REPEATING)
        self.assertEqual(self.resolve("agentsfleetd/state/vault.zig", "loadMetadata").cls,
                         trace.CLASS_ARENA)

    def test_the_walk_passes_through_an_inflated_middle_file(self) -> None:
        """`state/tenant_provider.zig` in miniature.

        The middle file is long-lived by label — a type import nothing calls
        keeps it there — but the only function reaching the leaf is itself called
        from a handler. One hop said leak-capable; the walk must say arena.
        """
        write(self.src, "agentsfleetd/http/handlers/thing.zig", """
            const mid = @import("../../state/mid.zig");
            fn handle() void { mid.resolve(); }
        """)
        write(self.src, "agentsfleetd/state/mid.zig", """
            const leaf = @import("leaf.zig");
            pub fn resolve() void { leaf.probe(); }
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            pub fn probe() void { errdefer cleanup(); }
        """)
        verdict = self.resolve("agentsfleetd/state/leaf.zig", "probe")
        self.assertEqual(verdict.cls, trace.CLASS_ARENA)
        self.assertEqual(
            [s.label(str(self.src)) for s in verdict.chain],
            [
                "agentsfleetd/http/handlers/thing.zig:handle",
                "agentsfleetd/state/mid.zig:resolve",
                "agentsfleetd/state/leaf.zig:probe",
            ],
        )

    def test_a_private_helper_is_reached_through_its_own_file(self) -> None:
        """114 of 130 `unreached` rungs were this, and none of them were dead.

        A private helper has no cross-file caller at all: the `pub fn` beside it
        does the calling. A walk that skips the function's own file calls that
        helper unreachable and drops it out of the sweep.
        """
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const leaf = @import("../state/leaf.zig");
            fn tick() void { leaf.entry(); }
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            pub fn entry() void { helper(); }
            fn helper() void { errdefer cleanup(); }
        """)
        self.assertEqual(self.resolve("agentsfleetd/state/leaf.zig", "helper").cls,
                         trace.CLASS_REPEATING)

    def test_a_method_call_inside_the_file_counts_too(self) -> None:
        """`self.helper()` is a call, and the walk cannot tell it from any other."""
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const leaf = @import("../state/leaf.zig");
            fn tick() void { leaf.entry(); }
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            pub fn entry(self: Leaf) void { self.helper(); }
            fn helper(self: Leaf) void { errdefer cleanup(); }
        """)
        self.assertEqual(self.resolve("agentsfleetd/state/leaf.zig", "helper").cls,
                         trace.CLASS_REPEATING)

    def test_the_worst_branch_wins(self) -> None:
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const leaf = @import("../state/leaf.zig");
            fn tick() void { leaf.probe(); }
        """)
        write(self.src, "agentsfleetd/http/handlers/thing.zig", """
            const leaf = @import("../../state/leaf.zig");
            fn handle() void { leaf.probe(); }
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            pub fn probe() void { errdefer cleanup(); }
        """)
        self.assertEqual(self.resolve("agentsfleetd/state/leaf.zig", "probe").cls,
                         trace.CLASS_REPEATING)

    def test_a_lost_trail_falls_back_to_the_file_class_and_says_so(self) -> None:
        """The one error direction that would hide a leak, refused.

        `fleet/service.zig:resolveProviderForLease` is called through a shape no
        regex follows, so the walk runs out of callers with the question open.
        Calling that branch dead would drop a rung out of the sweep; it inherits
        the caller file's own class instead, and the verdict is marked degraded.
        """
        write(self.src, "agentsfleetd/state/mid.zig", """
            const leaf = @import("leaf.zig");
            pub fn orphan() void { leaf.probe(); }
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            pub fn probe() void { errdefer cleanup(); }
        """)
        verdict = self.resolve(
            "agentsfleetd/state/leaf.zig",
            "probe",
            {"agentsfleetd/state/mid.zig": trace.CLASS_REPEATING},
        )
        self.assertEqual(verdict.cls, trace.CLASS_REPEATING)
        self.assertTrue(verdict.degraded)

    def test_nothing_calling_the_proven_function_is_unresolved_not_inherited(self) -> None:
        """The fallback is for a broken trail, not for a function with no callers.

        Inheriting the file class at the top of the walk would hand back the
        inflated label the walk exists to replace. `UNRESOLVED` sends the author
        to the call sites instead.
        """
        write(self.src, "agentsfleetd/state/leaf.zig", """
            pub fn probe() void { errdefer cleanup(); }
        """)
        verdict = self.resolve(
            "agentsfleetd/state/leaf.zig",
            "probe",
            {"agentsfleetd/state/leaf.zig": trace.CLASS_REPEATING},
        )
        self.assertEqual(verdict.cls, trace.CLASS_UNREACHED)
        self.assertFalse(verdict.degraded)

    def test_a_clean_walk_is_not_marked_degraded(self) -> None:
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const leaf = @import("../state/leaf.zig");
            fn tick() void { leaf.probe(); }
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            pub fn probe() void { errdefer cleanup(); }
        """)
        self.assertFalse(self.resolve("agentsfleetd/state/leaf.zig", "probe").degraded)

    def test_a_cycle_terminates(self) -> None:
        write(self.src, "agentsfleetd/state/a.zig", """
            const b = @import("b.zig");
            pub fn fa() void { b.fb(); }
        """)
        write(self.src, "agentsfleetd/state/b.zig", """
            const a = @import("a.zig");
            pub fn fb() void { a.fa(); }
        """)
        self.assertEqual(self.resolve("agentsfleetd/state/a.zig", "fa").cls,
                         trace.CLASS_UNREACHED)

    def test_a_function_in_a_root_file_needs_no_walk(self) -> None:
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            pub fn tick() void { errdefer cleanup(); }
        """)
        self.assertEqual(self.resolve("agentsfleetd/cron/FireService.zig", "tick").cls,
                         trace.CLASS_REPEATING)

    def test_the_enclosing_function_of_a_call_is_the_innermost_one(self) -> None:
        """A nested `fn` inside a struct performs the call, not the file."""
        body = textwrap.dedent("""
            pub fn outer() void {
                const Inner = struct {
                    fn inner() void { leaf.probe(); }
                };
                _ = Inner;
            }
        """)
        spans = edges.function_spans(body)
        self.assertEqual(edges.enclosing_fn(spans, body.index("leaf.probe")), "inner")


if __name__ == "__main__":
    unittest.main()
