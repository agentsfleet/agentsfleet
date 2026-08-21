#!/usr/bin/env python3
"""Self-test for the call-edge rules the rung classifier is built on.

An import taken for a type once marked five `state/**` files long-lived and cost
three proofs, so these cases pin exactly which imports count as edges — including
the two that must KEEP an edge, because over-keeping wastes proof effort while
over-pruning hides a leak.

`callers_of` gets its own cases: it is what a proof author runs before writing a
line, and the answer it gives has to be per FUNCTION where the class label is
only ever per file.
"""

from __future__ import annotations

import os
import tempfile
import textwrap
import unittest
from pathlib import Path

import rung_call_edges as edges


def write(root: Path, rel: str, body: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(body), encoding="utf-8")


class RungCallEdgesTest(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.src = Path(self._tmp.name) / "src"
        self.src.mkdir()
        self.addCleanup(self._tmp.cleanup)

    def sources(self) -> list[str]:
        return [f for f in edges.zig_sources(str(self.src)) if not edges.is_test_path(f)]

    def rel(self, path: str) -> str:
        return os.path.relpath(path, self.src)

    def assertEdge(self, importer: str, target: str) -> None:
        graph = edges.import_graph(self.sources())
        self.assertIn(str(self.src / target), graph[str(self.src / importer)])

    def assertNoEdge(self, importer: str, target: str) -> None:
        graph = edges.import_graph(self.sources())
        self.assertNotIn(str(self.src / target), graph[str(self.src / importer)])

    def test_type_only_import_is_not_an_edge(self) -> None:
        """The bug that cost three proofs, in eight lines.

        `semconv.zig` took ONE enum out of `tenant_provider.zig` and that alone
        marked five `state/**` files long-lived. An import graph cannot tell a
        type reference from a call; this rule can.
        """
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const Mode = @import("../state/mode.zig").Mode;
            fn tick(m: Mode) void { _ = m; }
        """)
        write(self.src, "agentsfleetd/state/mode.zig", """
            pub const Mode = enum { platform, self_managed };
            fn load() void {
                errdefer cleanup();
            }
        """)
        self.assertNoEdge("agentsfleetd/cron/FireService.zig", "agentsfleetd/state/mode.zig")

    def test_constant_only_import_is_not_an_edge(self) -> None:
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const limits = @import("../state/limits.zig");
            fn tick() void { _ = limits.MAX_ROWS; }
        """)
        write(self.src, "agentsfleetd/state/limits.zig", """
            pub const MAX_ROWS: usize = 100;
            fn load() void {
                errdefer cleanup();
            }
        """)
        self.assertNoEdge("agentsfleetd/cron/FireService.zig", "agentsfleetd/state/limits.zig")

    def test_a_type_carrying_methods_keeps_its_edge(self) -> None:
        """The limit of the rule, pinned so nobody tightens past it.

        `bearer_or_api_key.zig` holds `cli: ?*CliCredential` and calls
        `cli.execute(...)`. The alias never appears in a call position, so a rule
        keyed on call syntax alone would drop an edge that runs real code. A type
        that carries methods keeps its edge however the instance is obtained.
        """
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const cred_mod = @import("../auth/cred.zig");
            const Cred = cred_mod.Cred;
            const Holder = struct {
                cred: ?*Cred = null,
                fn run(self: Holder) void {
                    if (self.cred) |c| c.execute();
                }
            };
        """)
        write(self.src, "agentsfleetd/auth/cred.zig", """
            pub const Cred = struct {
                pub fn execute(self: Cred) void {
                    errdefer cleanup();
                    _ = self;
                }
            };
        """)
        self.assertEdge("agentsfleetd/cron/FireService.zig", "agentsfleetd/auth/cred.zig")

    def test_a_type_handed_only_to_a_comptime_builtin_is_pruned(self) -> None:
        """`@typeInfo(Mode)` builds no value, so none of `Mode`'s methods run."""
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const Mode = @import("../state/mode.zig").Mode;
            const COUNT: usize = @typeInfo(Mode).@"enum".fields.len;
        """)
        write(self.src, "agentsfleetd/state/mode.zig", """
            pub const Mode = enum {
                platform,
                pub fn label(self: Mode) []const u8 {
                    _ = self;
                    return "platform";
                }
            };
            fn load() void {
                errdefer cleanup();
            }
        """)
        self.assertNoEdge("agentsfleetd/cron/FireService.zig", "agentsfleetd/state/mode.zig")

    def test_a_type_listed_in_a_type_literal_is_pruned(self) -> None:
        """`[_]type{ ..., Mode, ... }` is a list of types, not a list of calls."""
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const Mode = @import("../state/mode.zig").Mode;
            const ENUMS = [_]type{ Mode };
        """)
        write(self.src, "agentsfleetd/state/mode.zig", """
            pub const Mode = enum {
                platform,
                pub fn label(self: Mode) []const u8 {
                    _ = self;
                    return "platform";
                }
            };
            fn load() void {
                errdefer cleanup();
            }
        """)
        self.assertNoEdge("agentsfleetd/cron/FireService.zig", "agentsfleetd/state/mode.zig")

    def test_an_unparsed_import_keeps_its_edge(self) -> None:
        """Over-keeping wastes proof effort; over-pruning hides a leak."""
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            fn tick() void { @import("../state/leaf.zig").f(); }
        """)
        write(self.src, "agentsfleetd/state/leaf.zig", """
            fn f() void {
                errdefer cleanup();
            }
        """)
        self.assertEdge("agentsfleetd/cron/FireService.zig", "agentsfleetd/state/leaf.zig")

    def test_pruned_edges_are_reported(self) -> None:
        """A change that shrinks the work list owes an audit trail."""
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const Mode = @import("../state/mode.zig").Mode;
            fn tick(m: Mode) void { _ = m; }
        """)
        write(self.src, "agentsfleetd/state/mode.zig", """
            pub const Mode = enum { platform };
        """)
        pruned: list[tuple[str, str]] = []
        edges.import_graph(self.sources(), pruned)
        self.assertEqual(
            [(str(self.src / "agentsfleetd/cron/FireService.zig"),
              str(self.src / "agentsfleetd/state/mode.zig"))],
            pruned,
        )

    def test_callers_of_answers_per_function_not_per_file(self) -> None:
        """The gap the class label cannot close.

        One file, two functions: a worker calls one and a handler calls the
        other. The FILE is `repeating` and always will be; only the per-function
        answer separates the rung worth proving from the cosmetic one.
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
            pub fn loadJson() void {
                errdefer cleanup();
            }
            pub fn loadMetadata() void {
                errdefer cleanup();
            }
        """)
        files = self.sources()
        target = str(self.src / "agentsfleetd/state/vault.zig")
        self.assertEqual(
            [self.rel(c) for c in edges.callers_of(files, target, "loadJson")],
            ["agentsfleetd/cron/FireService.zig"],
        )
        self.assertEqual(
            [self.rel(c) for c in edges.callers_of(files, target, "loadMetadata")],
            ["agentsfleetd/http/handlers/thing.zig"],
        )

    def test_callers_of_ignores_a_file_that_only_imports_the_type(self) -> None:
        write(self.src, "agentsfleetd/cron/FireService.zig", """
            const Row = @import("../state/vault.zig").Row;
            fn tick(r: Row) void { _ = r; }
        """)
        write(self.src, "agentsfleetd/state/vault.zig", """
            pub const Row = struct { id: u64 };
            pub fn loadJson() void {
                errdefer cleanup();
            }
        """)
        target = str(self.src / "agentsfleetd/state/vault.zig")
        self.assertEqual(edges.callers_of(self.sources(), target, "loadJson"), [])


if __name__ == "__main__":
    unittest.main()
