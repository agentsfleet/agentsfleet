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
Adding an entry is a code-review surface — every carve-out has a TODO
pointing at the spec that will rename the offending path.

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
    "messages",          # chat messages collection (per-zombie ingress)
    "memories",          # memory entries collection (per-zombie scratchpad)
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
    "charges",           # credit-pool charge rows (receive + stage per event)
    "diagnostics",       # tenant doctor block (provider posture, resolver state)
    "provider",          # tenant's currently-active LLM provider (singleton resource — exactly one row per tenant in core.tenant_providers)
    "integration-grants",
    "integration-requests",
    "approvals",         # approval-gate inbox collection
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
    "github",            # webhook subpath for GitHub Actions ingest (vendor-named provider)
    "complete",          # auth session completion (vendor-style verb, fine inbound)
    # Memory ops — top-level RPCs without a resource shape, predating §1:
    "forget", "list", "recall", "store",
    # Internal / non-public surface:
    "execute",           # /v1/execute internal RPC
}

# Vendor-immortal path carve-outs — these URLs are registered with an
# external vendor (Slack, GitHub, etc.) and renaming them breaks OAuth /
# webhooks until the vendor-side configuration is updated. They are NOT
# pending-rename items; they are external contracts that pin the path
# regardless of internal hygiene preferences. Per AGENTS.md (RULE NLG):
# vendor-immortal carve-outs are a separate class from deferred-cleanup
# tracking lists, and must be named explicitly so the distinction from
# "we'll get to it" is mechanical.
VENDOR_PATH_CARVE_OUTS: set[str] = set()

# A "noun-shaped" final segment is either a path param ({foo}), a colon-op
# (:approve, :reject), or matches the noun grammar: lowercase letters,
# digits, hyphens, optional trailing 's' or 'es' (we don't enforce
# pluralization mechanically — the allowlist carries the policy).
PATH_PARAM_RE = re.compile(r"^\{[a-zA-Z_][a-zA-Z0-9_]*\}$")
COLON_OP_RE = re.compile(r"^:[a-z][a-z0-9-]*$")
# {gate_id}:approve form — colon-noun applied to a path parameter, the
# canonical REST §1 single-resource operation shape (e.g. /approvals/{id}:approve).
PARAM_COLON_OP_RE = re.compile(r"^\{[a-zA-Z_][a-zA-Z0-9_]*\}:[a-z][a-z0-9-]*$")


def final_segment(path: str) -> str:
    parts = [p for p in path.split("/") if p]
    return parts[-1] if parts else ""


def is_legal_shape(path: str, last: str) -> bool:
    if path in TOP_LEVEL_ALLOW:
        return True
    if path in VENDOR_PATH_CARVE_OUTS:
        return True
    if PATH_PARAM_RE.match(last):
        return True
    if COLON_OP_RE.match(last):
        return True
    if PARAM_COLON_OP_RE.match(last):
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
    stale_carve_outs: list[str] = []

    for path in paths:
        last = final_segment(path)
        if is_legal_shape(path, last):
            continue
        violations.append((path, last))

    # Catch carve-out rot: a vendor entry that no longer corresponds to a real
    # path. A stale vendor entry means we've stopped serving an OAuth callback
    # we claim to register with a vendor — a louder failure than a typo.
    actual = set(paths)
    for carve_out in VENDOR_PATH_CARVE_OUTS:
        if carve_out not in actual:
            stale_carve_outs.append(carve_out)

    if violations:
        print("REST §1 violation(s) — final URL segment must be a plural noun, "
              "{path-param}, or :verb colon-op.\n"
              "See docs/REST_API_DESIGN_GUIDELINES.md §1.\n", file=sys.stderr)
        for path, last in violations:
            print(f"  {path}", file=sys.stderr)
            print(f"    final segment: '{last}'", file=sys.stderr)
            print(f"    fix: rename to a plural-noun resource or use a colon-op "
                  f"(POST /{{id}}:{last}). Pre-v2.0 hygiene admits no "
                  f"deferral list — fix in place per RULE NLG.", file=sys.stderr)
            print(file=sys.stderr)

    if stale_carve_outs:
        print("VENDOR_PATH_CARVE_OUTS contains entries that no longer exist in "
              "openapi.json — remove them:\n", file=sys.stderr)
        for p in stale_carve_outs:
            print(f"  {p}", file=sys.stderr)
        print(file=sys.stderr)

    if violations or stale_carve_outs:
        return 1

    print(f"OK: openapi.json — {len(paths)} paths, all REST §1 compliant.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
