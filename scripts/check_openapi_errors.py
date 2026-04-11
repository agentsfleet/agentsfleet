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
                    schema_ref = (
                        content[REQUIRED_CONTENT_TYPE]
                        .get("schema", {})
                        .get("$ref", "")
                    )
                    if not schema_ref.endswith(REQUIRED_SCHEMA):
                        failures.append(
                            f"{method.upper()} {path} HTTP {code}: "
                            f"schema ref is '{schema_ref}', expected '#/components/schemas/{REQUIRED_SCHEMA}'"
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
