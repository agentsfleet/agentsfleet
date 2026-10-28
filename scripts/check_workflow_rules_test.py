#!/usr/bin/env python3
"""Each workflow rule refuses the regression it was written for.

    python3 scripts/check_workflow_rules_test.py

Builds a minimal set of workflows that satisfies every rule, then breaks one
thing at a time and asserts the lint names the rule that broke.
"""

from __future__ import annotations

import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

import check_workflow_rules as checker

NOTIFY_ACTION = """\
inputs:
  vault: {required: true}
  webhook-item: {required: false, default: discord-ci-webhook}
runs:
  using: composite
  steps:
    - shell: bash
      run: op read "op://${{ inputs.vault }}/${{ inputs.webhook-item }}/credential"
"""

NOTIFY_DEV = """\
      - name: Notify
        uses: ./.github/actions/notify-discord
        with:
          vault: v
"""

NOTIFY_RELEASE = NOTIFY_DEV + "          webhook-item: discord-release-webhook\n"


def collector(org: str, vault: str, env_name: str) -> str:
    return f"""\
      - name: Collector
        env:
          FLY_ORG: {org}
          APPROVAL_SIGNING_SECRET: op://${{{{ vars.{vault} }}}}/approval-signing-secret/credential
        run: |
          bash scripts/ensure_fly_app.sh --create-only "$APP"
          flyctl secrets set GRAFANA_TOKEN=x APPROVAL_SIGNING_SECRET="$APPROVAL_SIGNING_SECRET"
          ENV={env_name} ./playbooks/lib/platform_secret_sync.sh approval-signing
"""


GATE = """\
      - run: STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh
"""

RUNNER = """\
      - name: Deploy runner
        run: ./playbooks/lib/runner/deploy.sh
        env:
          ALLOW_VAULT_READS: "1"
"""

LOG_LEVEL = """\
      - run: |
          export AGENTSFLEET_LOG_LEVEL="info"
"""


def workflow(*steps: str) -> str:
    return "on: push\njobs:\n  deploy:\n    runs-on: ubuntu-latest\n    steps:\n" + "".join(steps)


def clean_tree() -> dict[str, str]:
    dev = collector("agentsfleet-dev", "VAULT_DEV", "dev")
    prod = collector("agentsfleet-prod", "VAULT_PROD", "prod")
    return {
        checker.NOTIFY_ACTION: NOTIFY_ACTION,
        f"{checker.WORKFLOWS_DIR}/deploy-dev.yml": workflow(GATE, NOTIFY_DEV),
        f"{checker.WORKFLOWS_DIR}/deploy-dev-fly.yml": workflow(dev, LOG_LEVEL, NOTIFY_DEV),
        f"{checker.WORKFLOWS_DIR}/deploy-dev-metal.yml": workflow(RUNNER, NOTIFY_DEV),
        f"{checker.WORKFLOWS_DIR}/release.yml": workflow(GATE, prod, RUNNER, NOTIFY_RELEASE),
        f"{checker.WORKFLOWS_DIR}/post-release.yml": workflow(NOTIFY_RELEASE),
    }


def lint_tree(files: dict[str, str]) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for relative_path, text in files.items():
            (root / relative_path).parent.mkdir(parents=True, exist_ok=True)
            (root / relative_path).write_text(text, encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
            code = checker.main([checker.lib.ROOT_FLAG, str(root)])
        return code, out.getvalue()


def seeded(path: str, old: str, new: str) -> dict[str, str]:
    files = clean_tree()
    key = f"{checker.WORKFLOWS_DIR}/{path}" if not path.startswith(".github") else path
    assert old in files[key], (path, old)
    files[key] = files[key].replace(old, new)
    return files


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_moved_rules_reject_seeded_violations(self) -> None:
        cases = {
            "workflow-discord-default": seeded(checker.NOTIFY_ACTION, "default: discord-ci-webhook", "default: discord-release-webhook"),
            "workflow-discord-release-route": seeded("post-release.yml", "discord-release-webhook", "discord-ci-webhook"),
            "workflow-discord-dev-route": seeded("deploy-dev-metal.yml", "vault: v\n", "vault: v\n          webhook-item: discord-release-webhook\n"),
            "workflow-deployment-stage": seeded("release.yml", "STAGE=deployment", "STAGE=bootstrap"),
            "workflow-no-workspace-pointer": seeded("deploy-dev.yml", "./playbooks/founding", "PLATFORM_ADMIN_WORKSPACE_ID=x ./playbooks/founding"),
            "workflow-connector-boot-secret": seeded("release.yml", 'APPROVAL_SIGNING_SECRET="$APPROVAL_SIGNING_SECRET"', "GITHUB_APP_ID=x"),
            "workflow-connector-signing-row": seeded("deploy-dev-fly.yml", "ENV=dev ./playbooks", "ENV=prod ./playbooks"),
            "workflow-daemon-log-level": seeded("deploy-dev-fly.yml", 'export AGENTSFLEET_LOG_LEVEL="info"', 'LOG_LEVEL="info"'),
            "workflow-collector-org": seeded("deploy-dev-fly.yml", "FLY_ORG: agentsfleet-dev", "FLY_ORG: agentsfleet-prod"),
            "workflow-vault-read-approval": seeded("release.yml", 'ALLOW_VAULT_READS: "1"', 'ALLOW_VAULT_READS: "0"'),
            "workflow-missing": {k: v for k, v in clean_tree().items() if not k.endswith("deploy-dev-metal.yml")},
        }
        for rule, files in cases.items():
            with self.subTest(rule=rule):
                code, output = lint_tree(files)
                self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f": {rule}: ", output)

    def test_secrets_set_before_the_app_exists_is_refused(self) -> None:
        files = clean_tree()
        key = f"{checker.WORKFLOWS_DIR}/release.yml"
        create = '          bash scripts/ensure_fly_app.sh --create-only "$APP"\n'
        files[key] = files[key].replace(create, "").replace(
            "ENV=prod ./playbooks", "bash scripts/ensure_fly_app.sh --create-only \"$APP\"\n          ENV=prod ./playbooks"
        )
        code, output = lint_tree(files)
        self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
        self.assertIn("release.yml:1: workflow-collector-order: ", output)

    def test_a_comment_naming_a_retired_secret_is_not_a_finding(self) -> None:
        files = seeded("release.yml", "      - name: Collector\n", "      # GITHUB_APP_ID retired with the app.\n      - name: Collector\n")
        code, output = lint_tree(files)
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_a_step_indented_differently_is_still_read(self) -> None:
        files = clean_tree()
        key = f"{checker.WORKFLOWS_DIR}/deploy-dev-metal.yml"
        files[key] = "on: push\njobs:\n  deploy:\n      runs-on: ubuntu-latest\n      steps:\n        - uses: ./.github/actions/notify-discord\n"
        files[key] += "        - run: ./playbooks/founding/05_runner_bootstrap_dev/deploy.sh\n          env: {ALLOW_VAULT_READS: \"1\"}\n"
        code, output = lint_tree(files)
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_a_runner_step_with_no_approval_is_named_and_a_deploy_with_no_runner_step_is_refused(self) -> None:
        code, output = lint_tree(seeded("deploy-dev-metal.yml", '        env:\n          ALLOW_VAULT_READS: "1"\n', ""))
        self.assertIn("deploy-dev-metal.yml:1: workflow-vault-read-approval: step `Deploy runner` runs the runner playbook", output)
        code, output = lint_tree(seeded("release.yml", "./playbooks/lib/runner/deploy.sh", "./scripts/other.sh"))
        self.assertIn("release.yml:1: workflow-vault-read-approval: no step runs the runner playbook", output)

    def test_an_unknown_argument_is_a_usage_error(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(checker.lib.EXIT_USAGE, checker.main(["--bogus"]))


if __name__ == "__main__":
    unittest.main()
