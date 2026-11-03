#!/usr/bin/env python3
"""The Rust-source scan finds a component name two types publish, and refuses
to pass over a tree with no Rust in it.

    python3 scripts/check_rust_source_test.py
"""

from __future__ import annotations

import contextlib
import io
import sys
import unittest

sys.dont_write_bytecode = True

import check_rust_source as checker

WIRE_RS = """\
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[expect(
    clippy::large_enum_variant,
    reason = "fixture"
)]
pub struct LeasePolicy {
    pub hosts: Vec<String>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = RunnerPolicy))]
pub(crate) enum Policy {
    Open,
}
"""

API_WIRE_RS = """\
// A comment line between items.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Workspace {
    pub id: String,
}

impl utoipa::ToSchema for Manual {}
"""


def clean_tree() -> dict[str, str]:
    return {
        f"{checker.SCHEMA_CRATES[0]}/policy.rs": WIRE_RS,
        f"{checker.SCHEMA_CRATES[1]}/workspace.rs": API_WIRE_RS,
    }


def lint_tree(files: dict[str, str]) -> tuple[int, str]:
    with checker.lib.fixture_repository(files) as root:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = checker.lib.run([checker.lib.ROOT_FLAG, str(root)], "rust", checker.lint)
        return code, out.getvalue()


def seeded(path: str, old: str, new: str) -> dict[str, str]:
    files = clean_tree()
    assert old in files[path], (path, old)
    files[path] = files[path].replace(old, new)
    return files


class OneComponentNameOneType(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_two_types_under_one_name_across_crates_are_a_finding(self) -> None:
        files = seeded(f"{checker.SCHEMA_CRATES[1]}/workspace.rs", "pub struct Workspace", "pub struct LeasePolicy")
        code, output = lint_tree(files)
        self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
        self.assertIn("`LeasePolicy` is also published at", output)

    def test_an_alias_collides_by_the_name_it_publishes(self) -> None:
        files = seeded(f"{checker.SCHEMA_CRATES[1]}/workspace.rs", "for Manual", "for RunnerPolicy")
        code, output = lint_tree(files)
        self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
        self.assertIn("`RunnerPolicy` is also published at", output)

    def test_a_scan_that_reads_no_derive_is_a_finding(self) -> None:
        code, output = lint_tree({"rustd/crates/afd_fleet/src/lib.rs": "pub fn lease() {}\n"})
        self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
        self.assertIn("no ToSchema derive was read", output)


class AnEmptyScanIsAFinding(unittest.TestCase):
    def test_a_tree_with_no_crate_source_is_a_finding(self) -> None:
        code, output = lint_tree({"README.md": "no Rust here\n", "rustd/Cargo.toml": "[workspace]\n"})
        self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
        self.assertIn(f"{checker.CRATES}:1: rust-source-read: ", output)

        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)


if __name__ == "__main__":
    unittest.main()
