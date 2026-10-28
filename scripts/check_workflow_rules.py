#!/usr/bin/env python3
"""Rules over the deploy workflows that actionlint has no opinion on.

actionlint proves a workflow is valid YAML that GitHub will run. These rules
prove it runs the right things in the right order: each was a real outage or a
silent no-op that passed actionlint first.

    python3 scripts/check_workflow_rules.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error. Workflows are
read through `yq` (YAML to JSON), so a rule sees steps, `env` maps and `run`
scripts as the runner does, whatever the indentation or comments around them.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
from typing import Iterator

sys.dont_write_bytecode = True

import check_source_lib as lib

YQ = "yq"
WORKFLOWS_DIR = ".github/workflows"
NOTIFY_ACTION = ".github/actions/notify-discord/action.yml"
NOTIFY_USES = "./.github/actions/notify-discord"
WEBHOOK_INPUT = "webhook-item"
DEV_WEBHOOK = "discord-ci-webhook"
RELEASE_WEBHOOK = "discord-release-webhook"
NOTIFY_SECRET_REF = "op://${{ inputs.vault }}/${{ inputs.webhook-item }}/credential"
RELEASE_NOTIFIERS = ("release.yml", "post-release.yml")
DEV_NOTIFIERS = ("deploy-dev.yml", "deploy-dev-fly.yml", "deploy-dev-metal.yml")
DEPLOYMENT_GATE = "STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh"
DEPLOYMENT_GATE_RUNNERS = ("deploy-dev.yml", "release.yml")
WORKSPACE_POINTER = "PLATFORM_ADMIN_WORKSPACE_ID"
POINTER_FREE = ("deploy-dev.yml", "deploy-dev-fly.yml", "release.yml")
SIGNING_ENV = "APPROVAL_SIGNING_SECRET"
SIGNING_PASS = 'APPROVAL_SIGNING_SECRET="$APPROVAL_SIGNING_SECRET"'
RETIRED_APP_SECRETS = re.compile(r"GITHUB_APP_ID|GITHUB_APP_PRIVATE_KEY")
# Each workflow family, the vault variable its secrets read and the env name
# its vault-row seeding passes.
FAMILIES = {"deploy-dev": ("VAULT_DEV", "dev"), "release": ("VAULT_PROD", "prod")}
DAEMON_LOG_LEVEL = 'AGENTSFLEET_LOG_LEVEL="info"'
DEAD_LOG_LEVEL = re.compile(r"^\s*LOG_LEVEL=", re.MULTILINE)
DEV_FLY = "deploy-dev-fly.yml"
# The collector stand-up: each deploy workflow and the Fly organisation it
# creates into. Development and production are separate organisations.
COLLECTOR_ORGS = {"deploy-dev-fly.yml": "agentsfleet-dev", "release.yml": "agentsfleet-prod"}
CREATE_CALL = "ensure_fly_app.sh --create-only"
SECRETS_CALL = "flyctl secrets set"
COLLECTOR_MARK = "GRAFANA"
# The runner playbooks read the vault, and playbooks/lib/common.sh refuses to
# unless the caller set this approval; a deploy step without it fails on a host
# with nothing changed, after the build has already spent its time.
VAULT_READ_APPROVAL = ("ALLOW_VAULT_READS", "1")
RUNNER_PLAYBOOK = re.compile(r"playbooks/(?:lib/runner|founding/0[58]_runner_bootstrap_\w+)/")
RUNNER_DEPLOYS = ("deploy-dev-metal.yml", "release.yml")


class YamlUnreadable(Exception):
    """yq could not turn a file into JSON."""


def load_yaml(path: Path) -> object:
    result = subprocess.run([YQ, "-o=json", ".", str(path)], capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise YamlUnreadable(f"{path}: {result.stderr.strip()}")
    return json.loads(result.stdout)


def steps(workflow: object) -> Iterator[dict]:
    jobs = workflow.get("jobs", {}) if isinstance(workflow, dict) else {}
    for job in jobs.values():
        for step in job.get("steps", []) if isinstance(job, dict) else []:
            if isinstance(step, dict):
                yield step


def scalars(node: object, key: str = "") -> Iterator[tuple[str, str]]:
    """Every string in the document with the key it sits under."""
    if isinstance(node, dict):
        for child_key, child in node.items():
            yield from scalars(child, str(child_key))
    elif isinstance(node, list):
        for child in node:
            yield from scalars(child, key)
    elif isinstance(node, str):
        yield key, node


def runs(document: object) -> str:
    return "\n".join(value for key, value in scalars(document) if key == "run")


class Workflows:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.findings: list[lib.Finding] = []
        self.cache: dict[str, object] = {}

    def load(self, relative_path: str) -> object | None:
        if relative_path not in self.cache:
            path = self.root / relative_path
            if not path.is_file():
                self.find(relative_path, "workflow-missing", "the file this rule reads is missing")
                self.cache[relative_path] = None
            else:
                self.cache[relative_path] = load_yaml(path)
        return self.cache[relative_path]

    def workflow(self, name: str) -> object | None:
        return self.load(f"{WORKFLOWS_DIR}/{name}")

    def family(self, prefix: str) -> list[object]:
        names = sorted(p.name for p in (self.root / WORKFLOWS_DIR).glob(f"{prefix}*.yml"))
        return [self.workflow(name) for name in names]

    def find(self, path: str, rule: str, message: str) -> None:
        self.findings.append(lib.Finding(path, 1, rule, message))


def notify_steps(workflow: object) -> list[dict]:
    return [step for step in steps(workflow) if step.get("uses") == NOTIFY_USES]


def check_discord_routes(w: Workflows) -> None:
    action = w.load(NOTIFY_ACTION)
    if action is not None:
        default = action.get("inputs", {}).get(WEBHOOK_INPUT, {}).get("default")
        if default != DEV_WEBHOOK or not any(NOTIFY_SECRET_REF in v for _, v in scalars(action)):
            w.find(NOTIFY_ACTION, "workflow-discord-default", f"the notify action must default {WEBHOOK_INPUT} to {DEV_WEBHOOK} and read it from the vault")
    for name in RELEASE_NOTIFIERS:
        found = notify_steps(w.workflow(name))
        selected = [s for s in found if s.get("with", {}).get(WEBHOOK_INPUT) == RELEASE_WEBHOOK]
        if len(found) != 1 or len(selected) != 1:
            w.find(f"{WORKFLOWS_DIR}/{name}", "workflow-discord-release-route", f"notify exactly once, selecting {RELEASE_WEBHOOK}")
    for name in DEV_NOTIFIERS:
        found = notify_steps(w.workflow(name))
        if len(found) != 1 or any(WEBHOOK_INPUT in s.get("with", {}) for s in found):
            w.find(f"{WORKFLOWS_DIR}/{name}", "workflow-discord-dev-route", f"notify exactly once, on the {DEV_WEBHOOK} default")


def check_deployment_stage(w: Workflows) -> None:
    for name in DEPLOYMENT_GATE_RUNNERS:
        if DEPLOYMENT_GATE not in runs(w.workflow(name)):
            w.find(f"{WORKFLOWS_DIR}/{name}", "workflow-deployment-stage", f"run `{DEPLOYMENT_GATE}`")
    for name in POINTER_FREE:
        if any(WORKSPACE_POINTER in value for _, value in scalars(w.workflow(name))):
            w.find(f"{WORKFLOWS_DIR}/{name}", "workflow-no-workspace-pointer", f"{WORKSPACE_POINTER} is a post-signup value no deploy may require")


def check_connector_secrets(w: Workflows) -> None:
    for prefix, (vault, env_name) in FAMILIES.items():
        documents = w.family(prefix)
        values = [pair for document in documents for pair in scalars(document)]
        script = "\n".join(runs(document) for document in documents)
        reference = f"op://${{{{ vars.{vault} }}}}/approval-signing-secret/credential"
        path = f"{WORKFLOWS_DIR}/{prefix}*.yml"
        if (SIGNING_ENV, reference) not in values or SIGNING_PASS not in script:
            w.find(path, "workflow-connector-boot-secret", f"load {SIGNING_ENV} from {reference} and pass it to Fly")
        if any(RETIRED_APP_SECRETS.search(value) for _, value in values):
            w.find(path, "workflow-connector-boot-secret", "the retired GitHub app boot secrets are still loaded")
        if f"ENV={env_name} ./playbooks/lib/platform_secret_sync.sh approval-signing" not in script:
            w.find(path, "workflow-connector-signing-row", "seed the approval-signing vault row the daemon reads")


def check_log_level(w: Workflows) -> None:
    script = runs(w.workflow(DEV_FLY))
    if DAEMON_LOG_LEVEL not in script or DEAD_LOG_LEVEL.search(script):
        w.find(f"{WORKFLOWS_DIR}/{DEV_FLY}", "workflow-daemon-log-level", f"set {DAEMON_LOG_LEVEL}, never the bare LOG_LEVEL nothing reads")


def check_collector_standup(w: Workflows) -> None:
    for name, org in COLLECTOR_ORGS.items():
        path = f"{WORKFLOWS_DIR}/{name}"
        standup = [
            step for step in steps(w.workflow(name))
            if SECRETS_CALL in step.get("run", "") and COLLECTOR_MARK in step.get("run", "")
        ]
        if not standup:
            w.find(path, "workflow-collector-order", f"no step calls `{SECRETS_CALL}` with the collector's credentials")
            continue
        script = standup[0]["run"]
        create_at, secrets_at = script.find(CREATE_CALL), script.find(SECRETS_CALL)
        if create_at == -1 or create_at > secrets_at:
            w.find(path, "workflow-collector-order", f"`{CREATE_CALL}` must run before `{SECRETS_CALL}`")
        if standup[0].get("env", {}).get("FLY_ORG") != org:
            w.find(path, "workflow-collector-org", f"the create path needs `FLY_ORG: {org}`")


def check_vault_read_approval(w: Workflows) -> None:
    key, value = VAULT_READ_APPROVAL
    for name in RUNNER_DEPLOYS:
        path = f"{WORKFLOWS_DIR}/{name}"
        runners = [step for step in steps(w.workflow(name)) if RUNNER_PLAYBOOK.search(step.get("run", ""))]
        if not runners:
            w.find(path, "workflow-vault-read-approval", "no step runs the runner playbook this rule guards")
        for step in runners:
            if str(step.get("env", {}).get(key)) != value:
                w.find(path, "workflow-vault-read-approval", f"step `{step.get('name', '?')}` runs the runner playbook without `{key}: \"{value}\"`")


RULES = (
    check_discord_routes, check_deployment_stage, check_connector_secrets, check_log_level, check_collector_standup,
    check_vault_read_approval,
)


def lint(root: Path) -> list[lib.Finding]:
    workflows = Workflows(root)
    for rule in RULES:
        rule(workflows)
    return workflows.findings


def main(argv: list[str]) -> int:
    if shutil.which(YQ) is None:
        print(f"✗ {YQ} not found. Install via: mise install yq", file=sys.stderr)
        return lib.EXIT_USAGE
    try:
        return lib.run(argv, "workflows", lint)
    except YamlUnreadable as error:
        print(f"✗ unreadable YAML: {error}", file=sys.stderr)
        return lib.EXIT_FINDINGS


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
