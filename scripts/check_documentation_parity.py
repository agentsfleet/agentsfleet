#!/usr/bin/env python3
"""Published prose states the values the code enforces.

Each rule pairs a value the code declares with the page that tells a reader
about it: a byte bound with the OpenAPI description that promises it, a scope
or role spelling with the contributor auth page, a connector's OAuth scopes
with the registration playbook an operator follows, the architecture page's
route tables with the routes the published document serves. The code is right
by definition; a finding means the prose went stale.

    python3 scripts/check_documentation_parity.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True

import check_source_lib as lib
import route_tables

OPENAPI = "public/openapi.json"
AUTH_PAGE = "docs/AUTH.md"
WIRE_SRC = "rustd/crates/afd_wire/src"
SCOPE_SRC = "rustd/crates/afd_auth/src/scope.rs"
ROLE_SRC = "rustd/crates/afd_tenant/src/workspace/access.rs"
ERROR_CODE_SRC = "rustd/crates/afd_core/src/error_code"
REGISTRY_SRC = "rustd/crates/afd_connector/src/registry/mod.rs"

RUST_CONST = re.compile(r"\bconst (?P<name>[A-Z][A-Z0-9_]*): usize = (?P<value>[0-9_ *+]+);")
RUST_STR_CONST = re.compile(r'\bconst (?P<name>[A-Z][A-Z0-9_]*): &str = "(?P<value>[^"]*)";')
ERROR_DECLARE = re.compile(r'\bconst (?P<name>[A-Z][A-Z0-9_]*): ErrorCode = ErrorCode::declare\("(?P<value>[^"]+)"\)')
WIRE_ARM = re.compile(r'Self::\w+ => "(?P<wire>[^"]+)"')
WIRE_FN = re.compile(r"fn wire\(self\) -> &'static str \{(?P<body>.*?)\n    \}", re.DOTALL)
# `Self::Jira => Archetype::Oauth2(Oauth2Flow { … scopes: "…" … })`: the
# provider, then its scope string, which a `\` continuation may wrap.
PROVIDER_SCOPES = re.compile(r'Self::(?P<provider>\w+) => Archetype::Oauth2\(.*?scopes: "(?P<scopes>(?:[^"\\]|\\.)*)"', re.DOTALL)
CONTINUATION = re.compile(r"\\\n\s*")
NO_NUL = "no NUL character"
UNKNOWN_FIELDS = "Unknown fields are refused"
OFFLINE_SCOPE = "offline_access"

ROLE_NAMES = ("ROLE_OWNER", "ROLE_MEMBER")
OWNER_ONLY = "AUTH_OWNER_ONLY"
CALL_ID_FRAMES = ("ToolCallStarted", "ToolCallProgress", "ToolCallCompleted")
TOOL_CALLS_POST = "/paths/~1v1~1runners~1me~1leases~1{lease_id}~1tool-calls/post/description"
TOOL_CALL_GET = (
    "/paths/~1v1~1workspaces~1{workspace_id}~1fleets~1{fleet_id}~1events~1{event_id}"
    "~1tool-calls~1{call_id}/get/description"
)
# The registration playbook for each OAuth provider, and the sentence shape
# that quotes its scopes there.
REGISTRATION = {
    "Jira": ("playbooks/operations/jira_app_registration/001_playbook.md", "Grant exactly `{}`"),
    "Linear": ("playbooks/operations/linear_app_registration/001_playbook.md", "The authorization request supplies `{}`."),
}


def evaluate(expression: str) -> int:
    """A Rust integer constant written as products and sums of literals."""
    return sum(
        eval_product(term) for term in expression.replace("_", "").split("+")
    )


def eval_product(term: str) -> int:
    value = 1
    for factor in term.split("*"):
        value *= int(factor.strip())
    return value


def flatten(text: str) -> str:
    return " ".join(text.split())


class Sources:
    def __init__(self, root: Path) -> None:
        self.root = root

    def text(self, relative_path: str) -> str:
        path = self.root / relative_path
        return path.read_text(encoding="utf-8") if path.is_file() else ""

    def tree(self, relative_dir: str) -> str:
        return "\n".join(p.read_text(encoding="utf-8") for p in lib.source_files(self.root / relative_dir, (".rs",)))

    def usize_consts(self) -> dict[str, int]:
        return {m["name"]: evaluate(m["value"]) for m in RUST_CONST.finditer(self.tree(WIRE_SRC))}


def pointer(document: object, path: str) -> str:
    node = document
    for part in path.lstrip("/").split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        node = node.get(part) if isinstance(node, dict) else None
    return flatten(node) if isinstance(node, str) else ""


def bound_cases(bounds: dict[str, int]) -> list[tuple[str, str, str]]:
    """(rule, JSON pointer, phrase the description must carry)."""
    def field(schema: str, prop: str) -> str:
        return f"/components/schemas/{schema}/properties/{prop}/description"

    def number(name: str) -> str:
        return str(bounds.get(name, f"<{name} undeclared>"))

    def span(name: str) -> str:
        return f"1 to {number(name)} bytes"

    cases = [("doc-call-id-bound", field(frame, "call_id"), span("CALL_ID_MAX_BYTES")) for frame in CALL_ID_FRAMES]
    cases += [("doc-tool-detail-bound", TOOL_CALLS_POST, number(n)) for n in ("DETAIL_POST_MAX_BYTES", "DETAIL_FIELD_MAX_BYTES")]
    cases.append(("doc-tool-detail-bound", TOOL_CALL_GET, number("DETAIL_FIELD_MAX_BYTES")))
    cases += [("doc-tool-detail-bound", field("ToolCallRecord", p), number("DETAIL_FIELD_MAX_BYTES")) for p in ("arguments", "output")]
    trace = {
        ("ToolTraceCall", "arguments"): ("ARGS_MAX_BYTES", "ARGS_LEAF_MAX_BYTES"),
        ("ToolTraceCall", "output_head"): ("OUTPUT_EDGE_MAX_LINES", "OUTPUT_EDGE_MAX_BYTES"),
        ("ToolTraceCall", "output_tail"): ("OUTPUT_EDGE_MAX_LINES", "OUTPUT_EDGE_MAX_BYTES"),
        ("ToolTrace", "calls"): ("TRACE_MAX_CALLS",),
    }
    cases += [("doc-tool-trace-bound", field(*where), number(n)) for where, names in trace.items() for n in names]
    cases.append(("doc-steer-bound", "/components/schemas/SteerRequest/description", UNKNOWN_FIELDS))
    for prop, name in (("message", "STEER_MESSAGE_MAX_BYTES"), ("operation_id", "OPERATION_ID_MAX_BYTES")):
        cases += [("doc-steer-bound", field("SteerRequest", prop), phrase) for phrase in (span(name), NO_NUL)]
    return cases


def check_openapi_bounds(sources: Sources) -> list[lib.Finding]:
    text = sources.text(OPENAPI)
    if not text:
        return [lib.Finding(OPENAPI, 1, "doc-openapi-missing", "the published document is missing")]
    document = json.loads(text)
    return [
        lib.Finding(OPENAPI, 1, rule, f"{where} must state `{phrase}`")
        for rule, where, phrase in bound_cases(sources.usize_consts())
        if phrase not in pointer(document, where)
    ]


def check_auth_page(sources: Sources) -> list[lib.Finding]:
    page = sources.text(AUTH_PAGE)
    wire_fn = WIRE_FN.search(sources.text(SCOPE_SRC))
    scopes = [m["wire"] for m in WIRE_ARM.finditer(wire_fn["body"])] if wire_fn else []
    if not scopes:
        return [lib.Finding(SCOPE_SRC, 1, "doc-auth-scope", "no scope wire spellings were read")]
    findings = [
        lib.Finding(AUTH_PAGE, 1, "doc-auth-scope", f"the scope catalogue does not list `{scope}`")
        for scope in scopes if scope not in page
    ]
    roles = {m["name"]: m["value"] for m in RUST_STR_CONST.finditer(sources.text(ROLE_SRC))}
    codes = {m["name"]: m["value"] for m in ERROR_DECLARE.finditer(sources.tree(ERROR_CODE_SRC))}
    named = [roles.get(name) for name in ROLE_NAMES] + [codes.get(OWNER_ONLY)]
    for spelling in named:
        if spelling is None or f"`{spelling}`" not in page:
            findings.append(lib.Finding(AUTH_PAGE, 1, "doc-auth-role", f"name `{spelling}` as the code spells it"))
    return findings


def check_registration_scopes(sources: Sources) -> list[lib.Finding]:
    declared = {
        m["provider"]: CONTINUATION.sub("", m["scopes"])
        for m in PROVIDER_SCOPES.finditer(sources.text(REGISTRY_SRC))
    }
    findings = []
    for provider, (playbook, sentence) in REGISTRATION.items():
        scopes = declared.get(provider)
        if scopes is None:
            findings.append(lib.Finding(REGISTRY_SRC, 1, "doc-registration-scope", f"no OAuth scopes declared for {provider}"))
            continue
        granted = " ".join(s for s in scopes.split(" ") if s != OFFLINE_SCOPE)
        if sentence.format(granted) not in flatten(sources.text(playbook)):
            findings.append(lib.Finding(playbook, 1, "doc-registration-scope", f"quote {provider}'s scopes as declared: `{granted}`"))
    return findings


def check_route_tables(sources: Sources) -> list[lib.Finding]:
    page = sources.text(route_tables.PAGE)
    text = sources.text(OPENAPI)
    if not page:
        return [lib.Finding(route_tables.PAGE, 1, route_tables.RULE_COUNT, "the architecture page is missing")]
    if not text:
        return []  # check_openapi_bounds already reports the missing document.
    return [
        lib.Finding(route_tables.PAGE, 1, rule, message)
        for rule, message in route_tables.findings(page, json.loads(text))
    ]


RULES = (check_openapi_bounds, check_auth_page, check_registration_scopes, check_route_tables)


def lint(root: Path) -> list[lib.Finding]:
    sources = Sources(root)
    return [finding for rule in RULES for finding in rule(sources)]


if __name__ == "__main__":
    raise SystemExit(lib.run(sys.argv[1:], "documentation parity", lint))
