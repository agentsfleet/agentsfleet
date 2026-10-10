#!/usr/bin/env python3
"""Each Rust-source rule refuses the drift it was written for.

    python3 scripts/check_rust_source_test.py
"""

from __future__ import annotations

import contextlib
import io
import json
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

MIGRATION_RS = 'pub static MIGRATIONS: &[Migration] = &[\n    migration!("100_schemas.sql"),\n    migration!("200_tenants.sql"),\n];\n'

KNOBS_RS = "pub(super) const SSE_MAX_STREAMS_DEFAULT: usize = 256;\n"

TAIL = {
    "series": {
        "ladder_streams": [64, 256, 4096],
        "streams_live": [64, 256, 4096],
        "heap_bytes_per_stream": [13000, 13494, 13600],
        "stream_receive_p95_ms": [20.0, 41.5, 180.0],
    },
    "measurements": {"frames_undelivered": 0, "streams_unreached": 0},
}

SKILL_MD = "---\nname: fixture\n---\nPost a summary comment on the pull request.\n"


def clean_tree() -> dict[str, str]:
    return {
        f"{checker.SCHEMA_CRATES[0]}/policy.rs": WIRE_RS,
        f"{checker.SCHEMA_CRATES[1]}/workspace.rs": API_WIRE_RS,
        checker.MIGRATION_LIST: MIGRATION_RS,
        "schema/100_schemas.sql": "CREATE SCHEMA core;\n",
        "schema/200_tenants.sql": "CREATE TABLE core.tenants ();\n",
        f"{checker.BUNDLES}/fixture/SKILL.md": SKILL_MD,
        f"{checker.BUNDLES}/fixture/TRIGGER.md": SKILL_MD,
        checker.KNOBS: KNOBS_RS,
        checker.TAIL_BASELINE: json.dumps(TAIL),
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


def baseline(**changes: object) -> dict[str, str]:
    tail = json.loads(json.dumps(TAIL))
    for key, value in changes.items():
        group = "measurements" if key in tail["measurements"] else "series"
        tail[group][key] = value
    files = clean_tree()
    files[checker.TAIL_BASELINE] = json.dumps(tail)
    return files


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_moved_rules_reject_seeded_violations(self) -> None:
        unlisted = clean_tree()
        unlisted["schema/300_vault.sql"] = "CREATE TABLE vault.secrets ();\n"
        cases = {
            "rust-schema-name": seeded(f"{checker.SCHEMA_CRATES[1]}/workspace.rs", "pub struct Workspace", "pub struct LeasePolicy"),
            "rust-migration-list": unlisted,
            "rust-bundle-retired-gate": seeded(f"{checker.BUNDLES}/fixture/TRIGGER.md", "Post a summary", "Wait for the Approval Card, then post a summary"),
            "rust-stream-ceiling": seeded(checker.KNOBS, "= 256;", "= 512;"),
        }
        for rule, files in cases.items():
            with self.subTest(rule=rule):
                code, output = lint_tree(files)
                self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f": {rule}: ", output)

    def test_an_alias_collides_by_the_name_it_publishes(self) -> None:
        files = seeded(f"{checker.SCHEMA_CRATES[1]}/workspace.rs", "for Manual", "for RunnerPolicy")
        code, output = lint_tree(files)
        self.assertIn("`RunnerPolicy` is also published at", output)

    def test_a_listed_file_git_does_not_track_is_refused(self) -> None:
        files = seeded(checker.MIGRATION_LIST, '    migration!("200_tenants.sql"),\n', '    migration!("200_tenants.sql"),\n    migration!("200_tenants.sql"),\n    migration!("900_draft.sql"),\n')
        code, output = lint_tree(files)
        self.assertIn("`900_draft.sql` is not a tracked schema file", output)
        self.assertIn("`200_tenants.sql` is listed twice", output)

    def test_a_ceiling_on_a_rung_that_missed_a_bound_is_refused(self) -> None:
        for files in (
            baseline(stream_receive_p95_ms=[20.0, 300.0, 180.0]),
            baseline(streams_live=[64, 200, 4096]),
            baseline(frames_undelivered=3),
            baseline(stream_receive_p95_ms=[20.0, 41.5]),
        ):
            with self.subTest():
                code, output = lint_tree(files)
                self.assertIn(": rust-stream-ceiling: ", output)

    def test_an_empty_scan_is_a_finding(self) -> None:
        files = {k: v for k, v in clean_tree().items() if not k.startswith("rustd/crates/afd")}
        code, output = lint_tree(files)
        self.assertIn("no ToSchema derive was read", output)
        self.assertIn("no migration entry or no tracked schema file was read", output)


if __name__ == "__main__":
    unittest.main()
