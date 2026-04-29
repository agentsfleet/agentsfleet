#!/usr/bin/env python3
"""M11_001 §3.1 — Verify openapi.json uses ErrorBody for all 4xx/5xx responses.

Spec acceptance criterion 7:
  "openapi.json uses $ref: ErrorBody for all error responses."

Exit 0 if valid, non-zero with failure description if not.
"""
import json
import sys

SPEC_PATH = "public/openapi.json"
REQUIRED_SCHEMA = "ErrorBody"
REQUIRED_CONTENT_TYPE = "application/problem+json"
OLD_SCHEMA = "Error"

# Explicit allowlist: (path, status_code) pairs that return a non-RFC-7807 body.
# Each entry MUST include a justification comment. These are structured status
# responses whose 503 shape equals their 200 shape — they are not errors in the
# RFC 7807 sense and shouldn't be forced into a problem+json document.
ALLOWLIST: set[tuple[str, str]] = {
    # /readyz 503: degraded readiness. Body is the same ReadyzBody schema as
    # the 200 (ready/database/queue booleans) with `ready: false` and the
    # failing dependency's boolean set to false. It's a readiness report, not
    # an error message, so the application/problem+json contract does not fit.
    ("/readyz", "503"),
}


def schema_uses_errorbody(node: dict, schemas: dict, seen: set | None = None) -> bool:
    """Return True iff `node` references ErrorBody directly or via allOf.
    Follows local component refs so endpoints can extend ErrorBody with
    structured fields (e.g. the approval-inbox 409 carries gate_id/outcome/
    resolved_by alongside the standard problem+json envelope) while still
    satisfying §3.1.
    """
    if seen is None:
        seen = set()
    ref = node.get("$ref", "")
    if ref.endswith(f"/{REQUIRED_SCHEMA}"):
        return True
    if ref.startswith("#/components/schemas/"):
        name = ref.rsplit("/", 1)[-1]
        if name in seen:
            return False
        seen.add(name)
        target = schemas.get(name)
        if isinstance(target, dict):
            return schema_uses_errorbody(target, schemas, seen)
    for branch in node.get("allOf", []) or []:
        if isinstance(branch, dict) and schema_uses_errorbody(branch, schemas, seen):
            return True
    return False


def main() -> int:
    try:
        with open(SPEC_PATH) as f:
            spec = json.load(f)
    except FileNotFoundError:
        print(f"FAIL: {SPEC_PATH} not found", file=sys.stderr)
        return 1

    schemas = spec.get("components", {}).get("schemas", {})
    failures = []

    # Check ErrorBody schema exists with required fields
    if REQUIRED_SCHEMA not in schemas:
        failures.append(f"components.schemas.{REQUIRED_SCHEMA} is missing")
    else:
        eb = schemas[REQUIRED_SCHEMA]
        required_fields = {"docs_uri", "title", "detail", "error_code", "request_id"}
        props = set(eb.get("properties", {}).keys())
        missing = required_fields - props
        if missing:
            failures.append(f"ErrorBody missing properties: {sorted(missing)}")

    # Check old Error schema is gone
    if OLD_SCHEMA in schemas:
        failures.append(
            f"components.schemas.Error still exists — should have been replaced by ErrorBody"
        )

    # Check shared Error response uses problem+json
    shared_error = spec.get("components", {}).get("responses", {}).get("Error", {})
    shared_content = shared_error.get("content", {})
    if REQUIRED_CONTENT_TYPE not in shared_content:
        failures.append(
            f"components.responses.Error does not use {REQUIRED_CONTENT_TYPE}"
        )

    # Check all explicit 4xx/5xx responses use problem+json
    for path, methods in spec.get("paths", {}).items():
        for method, op in methods.items():
            for code, resp in op.get("responses", {}).items():
                if not code.startswith(("4", "5")):
                    continue
                if (path, code) in ALLOWLIST:
                    continue
                content = resp.get("content", {})
                if not content:
                    # No inline content — must be $ref to shared Error response
                    ref = resp.get("$ref", "")
                    if ref and not ref.endswith("/responses/Error"):
                        failures.append(
                            f"{method.upper()} {path} HTTP {code}: "
                            f"uses $ref '{ref}', expected '#/components/responses/Error'"
                        )
                    continue
                if REQUIRED_CONTENT_TYPE not in content:
                    failures.append(
                        f"{method.upper()} {path} HTTP {code}: "
                        f"uses {list(content.keys())} instead of {REQUIRED_CONTENT_TYPE}"
                    )
                else:
                    schema_node = content[REQUIRED_CONTENT_TYPE].get("schema", {})
                    if not schema_uses_errorbody(schema_node, schemas):
                        ref = schema_node.get("$ref", "")
                        failures.append(
                            f"{method.upper()} {path} HTTP {code}: "
                            f"schema ref is '{ref}', expected ref to "
                            f"'#/components/schemas/{REQUIRED_SCHEMA}' "
                            f"(direct or under allOf)"
                        )

    if failures:
        print("FAIL: openapi.json error response violations (M11_001 §3.1):")
        for f in failures:
            print(f"  - {f}")
        return 1

    print(f"OK: openapi.json — {REQUIRED_SCHEMA} schema valid, all error responses use {REQUIRED_CONTENT_TYPE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
