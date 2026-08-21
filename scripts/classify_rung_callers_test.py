#!/usr/bin/env python3
"""Self-test for the rung caller classifier.

The classifier decides which `errdefer` rungs the sweep is graded on, so a rule
that silently misfiles a file moves the milestone's scope without moving any
code. These cases pin the decisions that are easy to get wrong: the arena
boundary must stop a long-lived root from claiming the handler tree, a file two
hops below a repeating root must still count, and a file no root reaches must
not be quietly folded into the arena set.
"""

from __future__ import annotations

import tempfile
import textwrap
import unittest
from pathlib import Path

import classify_rung_callers as classifier


def write(root: Path, rel: str, body: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(body), encoding="utf-8")


class ClassifyRungCallersTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.src = Path(self._tmp.name) / "src"
        self.src.mkdir()
        self.addCleanup(self._tmp.cleanup)

    def classify(self) -> dict[str, str]:
        return {r["file"]: r["class"] for r in classifier.classify(str(self.src))}

    def rungs(self) -> dict[str, int]:
        return {r["file"]: r["rungs"] for r in classifier.classify(str(self.src))}

    def test_handler_only_file_is_arena_even_when_boot_reaches_the_server(self) -> None:
        """The arena boundary is the point of the whole instrument.

        `cmd/` reaches the server, and the server dispatches to handlers. Without
        the cut at the handler tree every handler would inherit boot's lifetime
        and the distinction the milestone rests on would collapse to nothing.
        """
        write(self.src, "agentsfleetd/cmd/serve.zig", """
            const server = @import("../http/server.zig");
        """)
        write(self.src, "agentsfleetd/http/server.zig", """
            const h = @import("handlers/thing.zig");
        """)
        write(self.src, "agentsfleetd/http/handlers/thing.zig", """
            const helper = @import("helper.zig");
            fn f() void {
                errdefer cleanup();
            }
        """)
        write(self.src, "agentsfleetd/http/handlers/helper.zig", """
            fn g() void {
                errdefer cleanup();
            }
        """)
        got = self.classify()
        self.assertEqual(got["agentsfleetd/http/handlers/thing.zig"], classifier.CLASS_ARENA)
        self.assertEqual(got["agentsfleetd/http/handlers/helper.zig"], classifier.CLASS_ARENA)

    def test_transitive_reach_from_a_repeating_root_counts(self) -> None:
        """A rung two hops below the cron service leaks exactly as much as one hop."""
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const mid = @import("../state/mid.zig");
        """)
        write(self.src, "agentsfleetd/state/mid.zig", """
            const leaf = @import("leaf.zig");
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            fn f() void {
                errdefer cleanup();
            }
        """)
        self.assertEqual(
            self.classify()["agentsfleetd/state/leaf.zig"], classifier.CLASS_REPEATING
        )

    def test_repeating_wins_over_arena_when_both_reach(self) -> None:
        """A file both a handler and a worker reach can still leak. Severity wins."""
        write(self.src, "agentsfleetd/queue/redis_subscriber.zig", """
            const shared = @import("../state/shared.zig");
        """)
        write(self.src, "agentsfleetd/http/handlers/thing.zig", """
            const shared = @import("../../state/shared.zig");
        """)
        write(self.src, "agentsfleetd/state/shared.zig", """
            fn f() void {
                errdefer cleanup();
            }
        """)
        self.assertEqual(
            self.classify()["agentsfleetd/state/shared.zig"], classifier.CLASS_REPEATING
        )

    def test_file_no_root_reaches_is_unreached_not_arena(self) -> None:
        """`arena` means "proven reachable only under the arena", never "unknown"."""
        write(self.src, "lib/orphan.zig", """
            fn f() void {
                errdefer cleanup();
            }
        """)
        self.assertEqual(self.classify()["lib/orphan.zig"], classifier.CLASS_UNREACHED)

    def test_test_files_are_never_classified(self) -> None:
        """A proof's own rungs are not the product's rungs."""
        write(self.src, "agentsfleetd/state/thing_test.zig", """
            fn f() void {
                errdefer cleanup();
            }
        """)
        self.assertNotIn("agentsfleetd/state/thing_test.zig", self.classify())

    def test_indented_and_bare_errdefer_both_count_once(self) -> None:
        write(self.src, "agentsfleetd/cmd/thing.zig", """
            fn f() void {
                errdefer a();
                    errdefer b();
                const s = "errdefer not at line start";
            }
        """)
        self.assertEqual(self.rungs()["agentsfleetd/cmd/thing.zig"], 2)

    def test_leak_capable_expands_to_repeating_and_boot_once(self) -> None:
        self.assertEqual(
            set(classifier.LEAK_CAPABLE),
            {classifier.CLASS_REPEATING, classifier.CLASS_BOOT_ONCE},
        )


if __name__ == "__main__":
    unittest.main()
