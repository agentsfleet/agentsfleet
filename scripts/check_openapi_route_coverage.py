#!/usr/bin/env python3
"""REST §7 — every served public /v1 route must be documented in openapi.json.

`check_openapi_url_shape.py` and `check_openapi_errors.py` only validate paths
that are ALREADY in the spec. Nothing asserted the converse: that every route
the daemon actually serves has a spec entry at all. That gap is how the four
`/v1/admin/models` routes shipped live-but-undocumented for a whole milestone.

This script closes it. Source of truth for what is SERVED is the canonical
`Route` union in src/agentsfleetd/http/routes.zig ("All Route variants are
registered here"); source of truth for what is DOCUMENTED is the bundled
public/openapi.json, where every `$ref` is already resolved (RULE CIV — the
gate reads the bundle, never the split YAML, so a path reachable only through
a `$ref` still counts as documented).

Three checks, all read-only:
  A. Undocumented route — a served /v1 path that is neither in openapi.json
     nor in INTERNAL_ROUTE_ALLOW.
  B. Unaccounted variant — a Route variant this script cannot resolve to a
     path. A new variant whose trailing comment omits its served path fails
     here rather than being silently skipped, so coverage cannot rot by
     omission.
  C. Stale carve-out — an allowlist entry that no longer corresponds to a
     served route or a real variant. Dead exemptions are louder than typos.

Exit 0 if clean, non-zero with each violation listed. No arguments; run from
the repo root.
"""
import json
import re
import sys

SPEC_PATH = "public/openapi.json"
ROUTES_PATH = "src/agentsfleetd/http/routes.zig"

# The gate's remit: the public, versioned API surface. Non-/v1 served paths
# (k8s probes, the CDN-cached model-caps asset) are covered by NON_V1_VARIANTS.
V1_PREFIX = "/v1/"
ROUTE_UNION_HEADER = "pub const Route = union(enum) {"
ROUTE_UNION_FOOTER = "};"
PARAM_PLACEHOLDER = "{}"

# Served control-plane routes that are deliberately absent from the public
# spec. Every entry carries a one-line justification; adding one is a
# code-review surface. Keys are NORMALIZED paths (path params → `{}`).
INTERNAL_ROUTE_ALLOW: dict[str, str] = {
    "/v1/fleets/streams": "platform-admin introspection of live SSE streams on one instance; not a tenant-facing resource",
    "/v1/runners/me": "runner self-plane — identity read, authenticated by the runner bearer, never called by an API consumer",
    "/v1/runners/me/heartbeats": "runner self-plane — liveness beat on the runner↔control-plane protocol",
    "/v1/runners/me/leases": "runner self-plane — lease acquisition; the runner protocol, not the public API",
    "/v1/runners/me/leases/{}/activity": "runner self-plane — per-lease activity frames feeding the SSE tail",
    "/v1/runners/me/leases/{}/renew": "runner self-plane — lease extension + incremental metering",
    "/v1/runners/me/reports": "runner self-plane — terminal execution report (service_report.zig)",
    "/v1/runners/me/credentials/mint": "runner self-plane — on-demand credential mint; documenting it would advertise a token-minting surface",
    "/v1/runners/me/memory/{}": "runner self-plane — fleet memory hydrate/capture across the runner boundary",
    "/v1/runners/me/bundles/{}": "runner self-plane — content-hashed bundle fetch",
}

# Route variants whose routes.zig comment carries no served path, mapped to the
# path router.zig actually matches. Stated here rather than fixed at the source
# because editing routes.zig is outside this workstream's Files-Changed scope.
# Known rot vector: renaming one of these routes without updating this map goes
# unnoticed. The stale-key check below catches a REMOVED variant, not a renamed
# path — a routes.zig comment is the durable fix, tracked for a follow-up.
PATHLESS_VARIANT_PATHS: dict[str, str] = {
    "create_auth_session": "/v1/auth/sessions",
    "create_workspace": "/v1/workspaces",
    "approval_webhook": "/v1/webhooks/{fleet_id}/approval",
    "workspace_fleet_memories": "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories",
}

# Served variants outside the /v1 surface. They are documented in openapi.json
# already, but their shape is not `/v1/...`, so the coverage check skips them.
NON_V1_VARIANTS: dict[str, str] = {
    "healthz": "k8s liveness probe at /healthz",
    "readyz": "k8s readiness probe at /readyz",
    "metrics": "prometheus scrape at /metrics",
    "model_caps": "CDN-cached static catalogue at /_um/<hash>/cap.json (model_caps.zig MODEL_CAPS_PATH)",
}

# A union variant declaration: leading indentation (any amount — `zig fmt` uses
# four spaces, but matching `\s+` means a re-indent or a tab can't make a variant
# vanish from the scan, which would silently drop it from coverage), then `name:`
# (a payload-carrying variant) or `name,` (a bare one). A variant this misses
# would never reach the unaccounted-variant hard failure, defeating the gate's
# no-silent-undercover guarantee — so the match must be indentation-agnostic.
VARIANT_RE = re.compile(r"^\s+([a-z_][a-z0-9_]*)\s*[:,]")
# A `/v1/...` token, optionally followed by `|:alt` colon-op alternations —
# routes.zig writes the approvals pair as `{gate_id}:approve|:deny`.
PATH_RE = re.compile(r"/v1/[A-Za-z0-9_{}/:.\-]*(?:\|:[a-z][a-z0-9-]*)*")
PARAM_RE = re.compile(r"\{[^}]*\}")


def read_file(path: str) -> str | None:
    try:
        with open(path) as f:
            return f.read()
    except FileNotFoundError:
        return None


def normalize(path: str) -> str:
    """Erase path-parameter NAMES so `{ws}` (routes.zig) and `{workspace_id}`
    (openapi.json) compare equal. Only the shape is load-bearing here."""
    return PARAM_RE.sub(PARAM_PLACEHOLDER, path)


def expand_colon_ops(raw: str) -> list[str]:
    """`/a/{id}:approve|:deny` → both concrete colon-op paths."""
    parts = raw.split("|")
    paths = [parts[0]]
    if len(parts) > 1 and ":" in parts[0]:
        base = parts[0].rsplit(":", 1)[0]
        paths += [base + alt for alt in parts[1:] if alt.startswith(":")]
    return paths


def extract_paths(blob: str) -> list[str]:
    """Every served `/v1` path named in one variant's comment block. Trailing
    prose punctuation is stripped; a bare `/v1/webhooks/` (a doc-comment
    reference to a route FAMILY, not a route) is dropped."""
    out: list[str] = []
    for raw in PATH_RE.findall(blob):
        for path in expand_colon_ops(raw.rstrip(".,;")):
            if not path.endswith("/"):
                out.append(path)
    return out


def parse_route_variants(routes_text: str) -> dict[str, list[str]]:
    """Map each `Route` variant to the served paths named in its own comment
    block (the contiguous `//` lines above it, plus its trailing comment)."""
    body = routes_text.split(ROUTE_UNION_HEADER, 1)[1]
    variants: dict[str, list[str]] = {}
    pending: list[str] = []
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("//"):
            pending.append(stripped)
            continue
        match = VARIANT_RE.match(line)
        if match:
            variants[match.group(1)] = extract_paths(" ".join(pending) + " " + line)
        elif stripped == ROUTE_UNION_FOOTER:
            break
        pending = []
    return variants


class CarveOuts:
    """The three justified exemption tables, injectable so the self-tests can
    drive `collect_violations` against a crafted registry without the real
    allowlists firing their stale-entry checks."""

    def __init__(
        self,
        internal: dict[str, str] | None = None,
        pathless: dict[str, str] | None = None,
        non_v1: dict[str, str] | None = None,
    ) -> None:
        self.internal = INTERNAL_ROUTE_ALLOW if internal is None else internal
        self.pathless = PATHLESS_VARIANT_PATHS if pathless is None else pathless
        self.non_v1 = NON_V1_VARIANTS if non_v1 is None else non_v1


def served_index(variants: dict[str, list[str]], carve_outs: CarveOuts) -> tuple[dict[str, set[str]], list[str]]:
    """Normalized served path → the variants serving it, plus the variants this
    script could not account for at all (check B)."""
    served: dict[str, set[str]] = {}
    unaccounted: list[str] = []
    for name, paths in variants.items():
        if not paths:
            if name in carve_outs.pathless:
                paths = [carve_outs.pathless[name]]
            elif name in carve_outs.non_v1:
                continue
            else:
                unaccounted.append(name)
                continue
        for path in paths:
            served.setdefault(normalize(path), set()).add(name)
    return served, unaccounted


def find_undocumented(served: dict[str, set[str]], documented: set[str], carve_outs: CarveOuts) -> list[str]:
    violations = []
    for path in sorted(served):
        if path in documented or path in carve_outs.internal:
            continue
        owners = ", ".join(sorted(served[path]))
        violations.append(f"UNDOCUMENTED ROUTE: {path}  (routes.zig: {owners})")
    return violations


def find_stale_carve_outs(
    served: dict[str, set[str]], variants: dict[str, list[str]], carve_outs: CarveOuts
) -> list[str]:
    """An exemption that no longer names anything real is dead weight, and a
    dead runner-plane entry means we stopped serving a route we claim exists."""
    stale = []
    for path in sorted(carve_outs.internal):
        if path not in served:
            stale.append(f"STALE INTERNAL_ROUTE_ALLOW: {path} is no longer served")
    for name in sorted(carve_outs.pathless):
        if name not in variants:
            stale.append(f"STALE PATHLESS_VARIANT_PATHS: {name} is not a Route variant")
        elif variants[name]:
            stale.append(f"STALE PATHLESS_VARIANT_PATHS: {name} now carries a path comment — drop the manual entry")
    for name in sorted(carve_outs.non_v1):
        if name not in variants:
            stale.append(f"STALE NON_V1_VARIANTS: {name} is not a Route variant")
    return stale


def collect_violations(routes_text: str, spec: dict, carve_outs: CarveOuts | None = None) -> tuple[list[str], int]:
    carve_outs = carve_outs or CarveOuts()
    variants = parse_route_variants(routes_text)
    served, unaccounted = served_index(variants, carve_outs)
    documented = {normalize(p) for p in spec.get("paths", {})}

    violations = find_undocumented(served, documented, carve_outs)
    violations += [
        f"UNACCOUNTED VARIANT: {name} names no /v1 path in its comment — "
        f"add the served path as a trailing comment in {ROUTES_PATH}, or "
        f"justify it in PATHLESS_VARIANT_PATHS / NON_V1_VARIANTS"
        for name in sorted(unaccounted)
    ]
    violations += find_stale_carve_outs(served, variants, carve_outs)
    return violations, len(served)


def main() -> int:
    routes_text = read_file(ROUTES_PATH)
    if routes_text is None:
        print(f"FAIL: {ROUTES_PATH} not found", file=sys.stderr)
        return 1
    spec_text = read_file(SPEC_PATH)
    if spec_text is None:
        print(f"FAIL: {SPEC_PATH} not found — run `make openapi` first", file=sys.stderr)
        return 1

    violations, served_count = collect_violations(routes_text, json.loads(spec_text))

    if violations:
        print(
            f"Served-vs-documented parity violation(s) — every served public "
            f"{V1_PREFIX.rstrip('/')} route needs a {SPEC_PATH} entry or a "
            "justified allowlist carve-out.\n"
            "See docs/REST_API_DESIGN_GUIDELINES.md §7.\n",
            file=sys.stderr,
        )
        for violation in violations:
            print(f"  {violation}", file=sys.stderr)
        print(file=sys.stderr)
        return 1

    print(
        f"OK: route coverage — {served_count} served {V1_PREFIX.rstrip('/')} routes, "
        f"{len(INTERNAL_ROUTE_ALLOW)} internal carve-outs, all documented."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
