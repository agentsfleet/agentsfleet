#!/usr/bin/env python3
"""Each documentation-parity rule refuses prose that drifted from the code.

    python3 scripts/check_documentation_parity_test.py
"""

from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

import check_documentation_parity as checker

BOUNDS_RS = """\
pub const CALL_ID_MAX_BYTES: usize = 64;
pub const DETAIL_FIELD_MAX_BYTES: usize = 64 * 1024;
pub const DETAIL_POST_MAX_BYTES: usize = 256 * 1_024;
pub const ARGS_MAX_BYTES: usize = 2048;
pub const ARGS_LEAF_MAX_BYTES: usize = 256;
pub const OUTPUT_EDGE_MAX_LINES: usize = 5;
pub const OUTPUT_EDGE_MAX_BYTES: usize = 1024;
pub const TRACE_MAX_CALLS: usize = 200;
pub const OPERATION_ID_MAX_BYTES: usize = 200;
pub const STEER_MESSAGE_MAX_BYTES: usize = 8192;
"""

SCOPE_RS = """\
impl Scope {
    pub const fn wire(self) -> &'static str {
        match self {
            Self::FleetRead => "fleet:read",
            Self::RunnerSelf => "runner:self",
        }
    }
}
"""

ACCESS_RS = 'pub const ROLE_OWNER: &str = "owner";\npub const ROLE_MEMBER: &str = "member";\n'
ERROR_RS = 'pub const AUTH_OWNER_ONLY: ErrorCode = ErrorCode::declare("UZ-AUTH-026");\n'
AUTH_MD = "Scopes: `fleet:read`, `runner:self`. Roles: `owner`, `member`; a member meets `UZ-AUTH-026`.\n"

REGISTRY_RS = """\
            Self::Jira => Archetype::Oauth2(Oauth2Flow {
                scopes: "read:jira-work write:jira-work \\
                         offline_access",
            }),
            Self::Linear => Archetype::Oauth2(Oauth2Flow {
                scopes: "read,comments:create",
            }),
"""

JIRA_MD = "Grant exactly `read:jira-work\nwrite:jira-work`. The request also supplies offline access.\n"
LINEAR_MD = "The authorization request supplies `read,comments:create`.\n"


def described(text: str) -> dict:
    return {"description": text}


def openapi() -> dict:
    call_id = {"properties": {"call_id": described("A caller id, 1 to 64 bytes.")}}
    steer_field = described("1 to {} bytes, no NUL character.")
    return {
        "paths": {
            "/v1/runners/me/leases/{lease_id}/tool-calls": {"post": described("Up to 262144 bytes, 65536 per field.")},
            "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}/tool-calls/{call_id}": {
                "get": described("Each field holds 65536 bytes.")
            },
        },
        "components": {
            "schemas": {
                **{frame: call_id for frame in checker.CALL_ID_FRAMES},
                "ToolCallRecord": {"properties": {"arguments": described("65536 bytes"), "output": described("65536 bytes")}},
                "ToolTraceCall": {
                    "properties": {
                        "arguments": described("2048 bytes, 256 per leaf"),
                        "output_head": described("5 lines, 1024 bytes"),
                        "output_tail": described("5 lines, 1024 bytes"),
                    }
                },
                "ToolTrace": {"properties": {"calls": described("At most 200 calls.")}},
                "SteerRequest": {
                    "description": "Unknown fields are refused.",
                    "properties": {
                        "message": described(steer_field["description"].format(8192)),
                        "operation_id": described(steer_field["description"].format(200)),
                    },
                },
            }
        },
    }


def clean_tree() -> dict[str, str]:
    return {
        checker.OPENAPI: json.dumps(openapi(), indent=2),
        f"{checker.WIRE_SRC}/bounds.rs": BOUNDS_RS,
        checker.SCOPE_SRC: SCOPE_RS,
        checker.ROLE_SRC: ACCESS_RS,
        f"{checker.ERROR_CODE_SRC}/auth.rs": ERROR_RS,
        checker.AUTH_PAGE: AUTH_MD,
        checker.REGISTRY_SRC: REGISTRY_RS,
        checker.REGISTRATION["Jira"][0]: JIRA_MD,
        checker.REGISTRATION["Linear"][0]: LINEAR_MD,
    }


def lint_tree(files: dict[str, str]) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for relative_path, text in files.items():
            (root / relative_path).parent.mkdir(parents=True, exist_ok=True)
            (root / relative_path).write_text(text, encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = checker.lib.run([checker.lib.ROOT_FLAG, str(root)], "parity", checker.lint)
        return code, out.getvalue()


def seeded(path: str, old: str, new: str) -> dict[str, str]:
    files = clean_tree()
    assert old in files[path], (path, old)
    files[path] = files[path].replace(old, new)
    return files


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_moved_rules_reject_seeded_violations(self) -> None:
        cases = {
            "doc-call-id-bound": seeded(f"{checker.WIRE_SRC}/bounds.rs", "CALL_ID_MAX_BYTES: usize = 64", "CALL_ID_MAX_BYTES: usize = 128"),
            "doc-tool-detail-bound": seeded(f"{checker.WIRE_SRC}/bounds.rs", "64 * 1024", "32 * 1024"),
            "doc-tool-trace-bound": seeded(checker.OPENAPI, "At most 200 calls.", "At most two hundred calls."),
            "doc-steer-bound": seeded(checker.OPENAPI, "Unknown fields are refused.", "Extra fields are ignored."),
            "doc-auth-scope": seeded(checker.SCOPE_SRC, '"runner:self"', '"runner:enroll"'),
            "doc-auth-role": seeded(checker.AUTH_PAGE, "`member`", "member"),
            "doc-registration-scope": seeded(checker.REGISTRY_SRC, "read,comments:create", "read,write"),
            "doc-openapi-missing": {k: v for k, v in clean_tree().items() if k != checker.OPENAPI},
        }
        for rule, files in cases.items():
            with self.subTest(rule=rule):
                code, output = lint_tree(files)
                self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f": {rule}: ", output)

    def test_a_scope_wrapped_across_source_lines_is_read_whole(self) -> None:
        files = seeded(checker.REGISTRATION["Jira"][0], "write:jira-work`", "read:jira-user`")
        code, output = lint_tree(files)
        self.assertIn("quote Jira's scopes as declared: `read:jira-work write:jira-work`", output)

    def test_a_rewrapped_description_still_counts(self) -> None:
        files = seeded(checker.OPENAPI, "A caller id, 1 to 64 bytes.", "A caller id,\\n 1 to   64\\n bytes.")
        code, output = lint_tree(files)
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_an_integer_expression_evaluates(self) -> None:
        self.assertEqual(65536, checker.evaluate("64 * 1_024"))
        self.assertEqual(9, checker.evaluate("4 + 5"))


if __name__ == "__main__":
    unittest.main()
