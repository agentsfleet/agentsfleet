#!/usr/bin/env python3
"""Order-safety of the collector stand-up, asserted against the workflows.

An app must exist before `flyctl secrets set --app` addresses it, and the
create call is the only thing that makes that true on a fresh environment.
Ordering is the half neither actionlint nor review reliably catches: this exact
block was once inline in both deploy workflows, the two copies drifted within a
day, and one of them ran it AFTER the deploy it was supposed to precede. Both
copies passed actionlint. So the assertions here are about sequence and about
the variable the create path refuses without, not about YAML validity.

Reads the workflows as text rather than through a YAML parser: the subject is a
`run:` block's shell, which a parser hands back as one opaque string anyway, and
the repository ships no YAML dependency for a test to import.

    python3 scripts/ensure_fly_app_test.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

# The deploy workflows that stand a collector up, and the organisation each
# creates into. Development and production are separate Fly organisations, so
# there is no single right value and the script refuses rather than guessing —
# a production app created inside the development organisation is not an error
# Fly reports, it is one somebody finds later.
WORKFLOWS = {
    ".github/workflows/deploy-dev-fly.yml": "agentsfleet-dev",
    ".github/workflows/release.yml": "agentsfleet-prod",
}

CREATE_CALL = "ensure_fly_app.sh --create-only"
SECRETS_CALL = "flyctl secrets set"
STEP_START = re.compile(r"^      - name: (.+)$")


def repo_root() -> Path:
    """The worktree root, so the test runs from anywhere."""
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    )
    return Path(result.stdout.strip())


def collector_step(lines: list[str]) -> tuple[str, list[str]]:
    """The step that sets the collector's upstream credentials, and its body.

    Located by the call it makes rather than by its name: a step renamed in one
    workflow and not the other is exactly the drift this file exists to catch,
    and keying on the name would make that drift invisible here.
    """
    starts = [i for i, line in enumerate(lines) if STEP_START.match(line)]
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else len(lines)
        body = lines[start:end]
        if any(SECRETS_CALL in line and "GRAFANA" in "".join(body) for line in body):
            name = STEP_START.match(lines[start]).group(1)
            return name, body
    return "", []


def main() -> int:
    root = repo_root()
    passed = 0
    failed = 0

    def ok(name: str) -> None:
        nonlocal passed
        print(f"ok   {name}")
        passed += 1

    def bad(name: str, detail: str) -> None:
        nonlocal failed
        print(f"FAIL {name}\n       {detail}", file=sys.stderr)
        failed += 1

    for workflow, org in WORKFLOWS.items():
        path = root / workflow
        label = Path(workflow).name
        if not path.is_file():
            bad(f"test_collector_standup_is_order_safe[{label}]", "workflow is missing")
            continue

        lines = path.read_text(encoding="utf-8").splitlines()
        step_name, body = collector_step(lines)
        if not body:
            bad(
                f"test_collector_standup_is_order_safe[{label}]",
                f"no step calling `{SECRETS_CALL}` with the collector's credentials",
            )
            continue

        create_at = next((i for i, l in enumerate(body) if CREATE_CALL in l), None)
        secrets_at = next((i for i, l in enumerate(body) if SECRETS_CALL in l), None)

        if create_at is None:
            bad(
                f"test_collector_standup_is_order_safe[{label}]",
                f"step {step_name!r} never calls `{CREATE_CALL}`, so a first run "
                f"addresses an app that does not exist yet",
            )
        elif secrets_at is None or create_at > secrets_at:
            bad(
                f"test_collector_standup_is_order_safe[{label}]",
                f"`{CREATE_CALL}` runs after `{SECRETS_CALL}` in step "
                f"{step_name!r}; the app must exist before it can be addressed",
            )
        else:
            ok(f"test_collector_standup_is_order_safe[{label}]")

        # The create path refuses without an organisation, by design. A step
        # that creates but never names one fails at the refusal rather than at
        # the missing app, which is a worse error for the same cause.
        env_line = f"FLY_ORG: {org}"
        if any(env_line in line for line in body):
            ok(f"test_create_names_the_organisation_it_creates_into[{label}]")
        else:
            bad(
                f"test_create_names_the_organisation_it_creates_into[{label}]",
                f"step {step_name!r} calls the create path without `{env_line}`; "
                f"the script refuses rather than guessing an organisation",
            )

    print(f"\n{passed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
