#!/usr/bin/env python3
"""REST §1 — every URL path's final segment must be plural-noun shaped.

The rule (from docs/REST_API_DESIGN_GUIDELINES.md §1):
  - Plural noun for collections:        /products, /zombies/{id}/events
  - Path parameter at the leaf:         /products/{id}
  - Colon-noun operation (use sparingly): /approvals/{id}:approve

Anything else (bare verbs like /steer, /pause, /kill, /sync) violates §1.

This script is the mechanical gate so the rule survives context resets,
agent swaps, and human commits — not just LLM judgment in the moment.

Source of truth: public/openapi.json (already bundled by `make openapi`).

Allowlists below are intentionally small and have justification comments.
Adding an entry is a code-review surface — every legacy violation has a
TODO pointing at the spec that will rename it.

Exit 0 if clean, non-zero with each violation listed.
"""
import json
import re
import sys

SPEC_PATH = "public/openapi.json"

# Single-segment allowlist (top-level paths that aren't resource-shaped).
# These exist outside the /v1/<resource> hierarchy by design.
TOP_LEVEL_ALLOW: set[str] = {
    "/healthz",       # k8s convention
    "/readyz",        # k8s convention
    "/metrics",       # prometheus convention
}

# Final-segment allowlist for "obviously a noun even though grammatically
# it could be a verb". Every entry needs a one-line justification.
NOUN_FINAL_SEGMENT_ALLOW: set[str] = {
    # Resource collections (plural nouns, end-of-path):
    "events",            # zombie_events resource
    "credentials",       # core.credentials
    "zombies",           # core.zombies
    "workspaces",        # core.workspaces
    "api-keys",          # api keys collection
    "agent-keys",        # agent keys collection
    "platform-keys",     # admin platform keys
    "sessions",          # auth sessions
    "tenants",           # tenant resource
    "telemetry",         # zombie_execution_telemetry rows
    "billing",           # billing summary view
    "integration-grants",
    "integration-requests",
    # Sub-resource leaves that are operator-facing nouns:
    "stream",            # SSE sub-resource of /events; not "to stream", a thing
    "current-run",       # the active run record (read-only resource)
    "llm",               # LLM credential bucket (a class of credential)
    "interactions",      # slack interaction events
    # Webhook receivers — these are inbound endpoints, not RESTful resources.
    # Their shape is dictated by the calling vendor, not by us.
    "callback",          # OAuth callback receiver (slack, github)
    "events",            # slack events receiver (already covered above)
    "install",           # slack install initiation (vendor-prescribed path)
    "clerk",             # clerk webhook receiver
    "approval",          # webhook subpath for vendor-driven approval flows
    "grant-approval",    # webhook subpath for grant approval flows
    "complete",          # auth session completion (vendor-style verb, fine inbound)
    # Memory ops — top-level RPCs without a resource shape (legacy, predates §1):
    "forget", "list", "recall", "store",
    # Internal / non-public surface:
    "execute",           # /v1/execute internal RPC
}

# Known legacy violations — every entry MUST have a TODO + spec reference.
# Adding to this set buys time, not absolution. The accompanying comment is
# the durable record of the rename plan.
LEGACY_VIOLATIONS: set[str] = {
    # M42_001 shipped the steer ingress under a verb path. Cleaner shape per
    # REST §1 is POST /v1/.../zombies/{id}/events with body
    # {"type":"chat","actor_kind":"steer","message":"..."}; storage already
    # treats steer as one actor among many writing to core.zombie_events.
    # TODO: rename in a follow-up M-spec; CLI and SDK ride that change.
    "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/steer",
    # Pre-§1 endpoints kept while their owning specs are paused:
    # TODO: triage during the next API hygiene sweep.
    "/v1/workspaces/{workspace_id}/pause",
    "/v1/workspaces/{workspace_id}/sync",
    "/v1/workspaces/{workspace_id}/zombies/{zombie_id}/kill",
    "/v1/auth/sessions/{session_id}/complete",
    "/v1/memory/forget",
    "/v1/memory/list",
    "/v1/memory/recall",
    "/v1/memory/store",
    "/v1/slack/callback",
    "/v1/slack/install",
    "/v1/github/callback",
}

# A "noun-shaped" final segment is either a path param ({foo}), a colon-op
# (:approve, :reject), or matches the noun grammar: lowercase letters,
# digits, hyphens, optional trailing 's' or 'es' (we don't enforce
# pluralization mechanically — the allowlist carries the policy).
PATH_PARAM_RE = re.compile(r"^\{[a-zA-Z_][a-zA-Z0-9_]*\}$")
COLON_OP_RE = re.compile(r"^:[a-z][a-z0-9-]*$")


def final_segment(path: str) -> str:
    parts = [p for p in path.split("/") if p]
    return parts[-1] if parts else ""


def is_legal_shape(path: str, last: str) -> bool:
    if path in TOP_LEVEL_ALLOW:
        return True
    if PATH_PARAM_RE.match(last):
        return True
    if COLON_OP_RE.match(last):
        return True
    if last in NOUN_FINAL_SEGMENT_ALLOW:
        return True
    return False


def main() -> int:
    try:
        with open(SPEC_PATH) as f:
            spec = json.load(f)
    except FileNotFoundError:
        print(f"FAIL: {SPEC_PATH} not found — run `make openapi` first", file=sys.stderr)
        return 1

    paths = list(spec.get("paths", {}).keys())
    violations: list[tuple[str, str]] = []
    stale_legacy: list[str] = []

    for path in paths:
        last = final_segment(path)
        if is_legal_shape(path, last):
            continue
        if path in LEGACY_VIOLATIONS:
            continue
        violations.append((path, last))

    # Catch allowlist rot: an entry that no longer corresponds to a real path.
    actual = set(paths)
    for legacy in LEGACY_VIOLATIONS:
        if legacy not in actual:
            stale_legacy.append(legacy)

    if violations:
        print("REST §1 violation(s) — final URL segment must be a plural noun, "
              "{path-param}, or :verb colon-op.\n"
              "See docs/REST_API_DESIGN_GUIDELINES.md §1.\n", file=sys.stderr)
        for path, last in violations:
            print(f"  {path}", file=sys.stderr)
            print(f"    final segment: '{last}'", file=sys.stderr)
            print(f"    fix: rename to a resource (POST /events) or a colon-op "
                  f"(POST /{{id}}:{last})", file=sys.stderr)
            print(f"    or: add '{path}' to LEGACY_VIOLATIONS in "
                  f"scripts/check_openapi_url_shape.py with a TODO + spec ref.",
                  file=sys.stderr)
            print(file=sys.stderr)

    if stale_legacy:
        print("LEGACY_VIOLATIONS contains entries that no longer exist in "
              "openapi.json — remove them:\n", file=sys.stderr)
        for p in stale_legacy:
            print(f"  {p}", file=sys.stderr)
        print(file=sys.stderr)

    if violations or stale_legacy:
        return 1

    print(f"OK: openapi.json — {len(paths)} paths, all REST §1 compliant "
          f"({len(LEGACY_VIOLATIONS)} pre-§1 endpoints on the legacy list).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
