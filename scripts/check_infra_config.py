#!/usr/bin/env python3
"""Rules over deployment configuration that nothing executes before a deploy.

Each rule reads a configuration file through its format's parser and checks a
property a deploy depends on: the runner's health window outlasts its restart
delay, its drain waits as long as the unit's stop timeout, the tailnet policy grants CI exactly the worker SSH it needs, the Fly
apps stay private behind their tunnel and their pools fit the database.

    python3 scripts/check_infra_config.py [--root <dir>]

Exit 0 clean, 1 with one line per finding, 2 on a usage error.
"""

from __future__ import annotations

import configparser
import json
from pathlib import Path
import re
import shutil
import sys
import tomllib

sys.dont_write_bytecode = True

import check_source_lib as lib
import check_workflow_rules as workflows

# --- The health window outlasts the restart delay ----------------------------
RUNNER_UNIT = "deploy/baremetal/agentsfleet-runner.service"
DEPLOY_SH = "deploy/baremetal/deploy.sh"
HEALTH_WINDOW = re.compile(r"^(?:readonly )?HEALTH_STABLE_SECONDS=(?P<seconds>[0-9]+)$", re.MULTILINE)
# How long drain_runner waits for a stopping runner; systemd kills it at TimeoutStopSec.
DRAIN_TIMEOUT = re.compile(r"^(?:readonly )?DRAIN_TIMEOUT_SECONDS=(?P<seconds>[0-9]+)$", re.MULTILINE)
# systemd.time(7): a span is a sequence of numbers each with an optional unit;
# a bare number is seconds.
TIMESPAN_PART = re.compile(r"(?P<value>[0-9.]+)\s*(?P<unit>[a-z]*)")
TIMESPAN_UNITS = {
    "us": 1e-6, "usec": 1e-6, "ms": 1e-3, "msec": 1e-3,
    "": 1, "s": 1, "sec": 1, "second": 1, "seconds": 1,
    "m": 60, "min": 60, "minute": 60, "minutes": 60,
    "h": 3600, "hr": 3600, "hour": 3600, "hours": 3600,
    "d": 86400, "day": 86400, "days": 86400,
}

# --- The tailnet policy ------------------------------------------------------
TAILNET_POLICY = "playbooks/founding/02_preflight/tailnet-policy.hujson"
TAG_CI, TAG_WORKER = "tag:ci", "tag:worker"
ADMIN_GROUP, MEMBER_GROUP = "autogroup:admin", "autogroup:member"
ROOT_USER, SSH_PORT, WILDCARD = "root", "tcp:22", "*"
PLAYBOOKS = "playbooks"
CODE_FENCE = re.compile(r"^```[^\n]*\n(?P<body>.*?)^```", re.MULTILINE | re.DOTALL)
TAILSCALE_UP = re.compile(r"tailscale up\b(?P<flags>(?:[^\n\\]|\\\n)*)")
ADVERTISED = re.compile(r"--advertise-tags[= ](?P<tags>\S+)")

# --- The Fly topology --------------------------------------------------------
FLY_DIR = "deploy/fly"
HOSTNAMES = {"dev": "api-dev.agentsfleet.net", "prod": "api.agentsfleet.net"}
API_PORT = 3000
READINESS_PATH = "/readyz"
RESTART_POLICY = "always"
TUNNEL_FALLBACK = "http_status:404"
PUBLIC_SERVICE_KEYS = ("services", "http_service")
RELEASE = ".github/workflows/release.yml"
DESIRED_MACHINES = re.compile(r"^\s*DESIRED_API_MACHINES=(?P<count>[0-9]+)\s*$", re.MULTILINE)
SCALE_STEPS = (
    'flyctl scale count "$DESIRED_API_MACHINES"',
    'test "$TOTAL" -eq "$DESIRED_API_MACHINES"',
    'test "$RUNNING" -eq "$DESIRED_API_MACHINES"',
)
# `SHOW max_connections` on both PlanetScale clusters is 25: 3 are
# superuser-reserved and 3 held by pscale_admin and pscale_exporter, leaving
# 19, from which an operator session and the release migrator are kept clear.
# A fleet promising more backends is refused with 53300 and the daemon answers
# UZ-INTERNAL-001.
CLUSTER_BUDGET = 17
MIGRATOR_CONNECTIONS = 1
POOL_KNOB = "DATABASE_POOL_SIZE_API"
# Development runs two API machines; it has no DESIRED_API_MACHINES of its own.
DEV_REPLICAS = 2


def finding(path: str, rule: str, message: str) -> lib.Finding:
    return lib.Finding(path, 1, rule, message)


def read(root: Path, relative_path: str) -> str | None:
    path = root / relative_path
    return path.read_text(encoding="utf-8") if path.is_file() else None


def timespan_seconds(span: str) -> float | None:
    """A systemd time span in seconds, or None when it does not parse."""
    parts = list(TIMESPAN_PART.finditer(span.replace(" ", "")))
    if not parts or "".join(p.group(0) for p in parts) != span.replace(" ", ""):
        return None
    if any(p["unit"] not in TIMESPAN_UNITS for p in parts):
        return None
    return sum(float(p["value"]) * TIMESPAN_UNITS[p["unit"]] for p in parts)


def service_option(unit: str, key: str) -> str | None:
    """One `[Service]` setting of a systemd unit, read as configparser reads an INI file."""
    parser = configparser.ConfigParser(strict=False, interpolation=None)
    parser.optionxform = str
    parser.read_string(unit)
    return parser.get("Service", key, fallback=None)


def check_restart_window(root: Path) -> list[lib.Finding]:
    unit, deploy = read(root, RUNNER_UNIT), read(root, DEPLOY_SH)
    if unit is None or deploy is None:
        return [finding(RUNNER_UNIT, "infra-restart-window", f"{RUNNER_UNIT} and {DEPLOY_SH} must both exist")]
    span = service_option(unit, "RestartSec")
    restart = timespan_seconds(span) if span else None
    window = HEALTH_WINDOW.search(deploy)
    if restart is None or window is None:
        return [finding(RUNNER_UNIT, "infra-restart-window", f"RestartSec ({span!r}) or HEALTH_STABLE_SECONDS does not parse")]
    if int(window["seconds"]) <= restart:
        return [finding(DEPLOY_SH, "infra-restart-window", f"HEALTH_STABLE_SECONDS={window['seconds']} must outlast RestartSec={span}, or a crash loop passes verify_healthy")]
    return []


def check_drain_timeout(root: Path) -> list[lib.Finding]:
    unit, deploy = read(root, RUNNER_UNIT), read(root, DEPLOY_SH)
    if unit is None or deploy is None:
        return [finding(RUNNER_UNIT, "infra-drain-timeout", f"{RUNNER_UNIT} and {DEPLOY_SH} must both exist")]
    span = service_option(unit, "TimeoutStopSec")
    stop = timespan_seconds(span) if span else None
    drain = DRAIN_TIMEOUT.search(deploy)
    if stop is None or drain is None:
        return [finding(RUNNER_UNIT, "infra-drain-timeout", f"TimeoutStopSec ({span!r}) or DRAIN_TIMEOUT_SECONDS does not parse")]
    if int(drain["seconds"]) != stop:
        return [finding(DEPLOY_SH, "infra-drain-timeout", f"DRAIN_TIMEOUT_SECONDS={drain['seconds']} must equal TimeoutStopSec={span}: a shorter drain cuts a run the unit would let finish")]
    return []


def strip_hujson(text: str) -> str:
    """HuJSON to JSON: comments and trailing commas removed, strings untouched."""
    blanked = lib.blank_js_comments(text)
    out, quote = [], False
    for index, char in enumerate(blanked):
        if char == '"' and (index == 0 or blanked[index - 1] != "\\"):
            quote = not quote
        if char == "," and not quote and re.match(r"\s*[}\]]", blanked[index + 1 :]):
            continue
        out.append(char)
    return "".join(out)


def check_tailnet_policy(root: Path) -> list[lib.Finding]:
    text = read(root, TAILNET_POLICY)
    try:
        policy = json.loads(strip_hujson(text or ""))
    except ValueError as error:
        return [finding(TAILNET_POLICY, "infra-tailnet-policy", f"not parseable as HuJSON: {error}")]
    found = []
    owners = policy.get("tagOwners", {})
    if any(ADMIN_GROUP not in owners.get(tag, []) for tag in (TAG_CI, TAG_WORKER)):
        found.append(finding(TAILNET_POLICY, "infra-tailnet-tag-owners", f"{TAG_CI} and {TAG_WORKER} must each be owned by {ADMIN_GROUP}"))
    if not any(
        rule.get("action") == "accept" and TAG_CI in rule.get("src", []) and TAG_WORKER in rule.get("dst", []) and ROOT_USER not in rule.get("users", [])
        for rule in policy.get("ssh", [])
    ):
        found.append(finding(TAILNET_POLICY, "infra-tailnet-ci-ssh", f"ssh must accept {TAG_CI} -> {TAG_WORKER} for non-root users, or every CI deploy is refused"))
    grants = policy.get("grants", [])
    worker_ssh = [
        g for g in grants
        if {MEMBER_GROUP, TAG_CI} <= set(g.get("src", [])) and g.get("dst") == [TAG_WORKER] and g.get("ip") == [SSH_PORT]
    ]
    if len(worker_ssh) != 1 or any(WILDCARD in g.get(k, []) for g in grants for k in ("src", "dst", "ip")):
        found.append(finding(TAILNET_POLICY, "infra-tailnet-grant", f"one grant lets members and {TAG_CI} reach {TAG_WORKER} on {SSH_PORT}, and no grant is a wildcard"))
    if not any(
        test.get("src") == TAG_CI and TAG_WORKER in test.get("dst", []) and all(d.startswith("tag:") for d in test.get("dst", []))
        and test.get("accept") and ROOT_USER in test.get("deny", [])
        for test in policy.get("sshTests", [])
    ):
        found.append(finding(TAILNET_POLICY, "infra-tailnet-sshtests", f"sshTests must assert {TAG_CI} -> {TAG_WORKER} (tags only) for a non-root user, root denied"))
    return found


def check_worker_tag_commands(root: Path) -> list[lib.Finding]:
    found, commands = [], 0
    for path in sorted((root / PLAYBOOKS).rglob("*.md")):
        text = path.read_text(encoding="utf-8")
        for block in CODE_FENCE.finditer(text):
            for command in TAILSCALE_UP.finditer(block["body"]):
                commands += 1
                tags = [t for m in ADVERTISED.finditer(command["flags"]) for t in m["tags"].split(",")]
                if tags != [TAG_WORKER]:
                    line = lib.line_of(text, block.start("body") + command.start())
                    found.append(lib.Finding(lib.relative(root, path), line, "infra-worker-tag", f"a host joins with --advertise-tags={TAG_WORKER} only, never {TAG_CI}"))
    if commands == 0:
        found.append(finding(PLAYBOOKS, "infra-worker-tag", "no playbook joins a host with `tailscale up`; the scan is broken, not the tree"))
    return found


def load_toml(root: Path, relative_path: str) -> dict:
    text = read(root, relative_path)
    return tomllib.loads(text) if text is not None else {}


def check_environment(root: Path, environment: str, replicas: int) -> list[lib.Finding]:
    api_app, tunnel_app = f"agentsfleetd-{environment}", f"cloudflared-{environment}"
    api_path, tunnel_path = f"{FLY_DIR}/{api_app}/fly.toml", f"{FLY_DIR}/{tunnel_app}/fly.toml"
    config_path = f"{FLY_DIR}/{tunnel_app}/config.yml"
    api, tunnel = load_toml(root, api_path), load_toml(root, tunnel_path)
    found = []
    for path, config, app in ((api_path, api, api_app), (tunnel_path, tunnel, tunnel_app)):
        if config.get("app") != app:
            found.append(finding(path, "infra-fly-app", f"app must be {app}"))
        if not any(r.get("policy") == RESTART_POLICY for r in config.get("restart", [])):
            found.append(finding(path, "infra-fly-restart", f"restart policy must be {RESTART_POLICY}"))
        if any(key in config for key in PUBLIC_SERVICE_KEYS):
            found.append(finding(path, "infra-fly-private", "publishes a Fly service; traffic arrives through the tunnel only"))
    if not any(c.get("path") == READINESS_PATH and c.get("port") == API_PORT for c in api.get("checks", {}).values()):
        found.append(finding(api_path, "infra-fly-readiness", f"an HTTP check on port {API_PORT} at {READINESS_PATH}"))
    ingress = (workflows.load_yaml(root / config_path) or {}).get("ingress", []) if (root / config_path).is_file() else []
    origin = f"http://{api_app}.internal:{API_PORT}"
    routed = any(r.get("hostname") == HOSTNAMES[environment] and r.get("service") == origin for r in ingress)
    if not routed or not ingress or ingress[-1].get("service") != TUNNEL_FALLBACK:
        found.append(finding(config_path, "infra-tunnel-ingress", f"route {HOSTNAMES[environment]} to {origin}, then fall back to {TUNNEL_FALLBACK}"))
    pool = api.get("env", {}).get(POOL_KNOB)
    if pool is None or not str(pool).isdigit():
        found.append(finding(api_path, "infra-pool-budget", f"set {POOL_KNOB}; the code default exceeds the cluster"))
    elif int(pool) * replicas + MIGRATOR_CONNECTIONS > CLUSTER_BUDGET:
        found.append(finding(api_path, "infra-pool-budget", f"{replicas} machines x {pool} + migrator exceeds the {CLUSTER_BUDGET} connections the cluster allows"))
    return found


def check_fly_topology(root: Path) -> list[lib.Finding]:
    release = workflows.runs(workflows.load_yaml(root / RELEASE)) if (root / RELEASE).is_file() else ""
    desired = DESIRED_MACHINES.search(release)
    found = [finding(RELEASE, "infra-fly-scale", f"scale and verify the production API with `{step}`") for step in SCALE_STEPS if step not in release]
    if desired is None:
        return found + [finding(RELEASE, "infra-fly-scale", "release.yml declares no DESIRED_API_MACHINES")]
    return found + check_environment(root, "dev", DEV_REPLICAS) + check_environment(root, "prod", int(desired["count"]))


RULES = (check_restart_window, check_drain_timeout, check_tailnet_policy, check_worker_tag_commands, check_fly_topology)


def lint(root: Path) -> list[lib.Finding]:
    return [f for rule in RULES for f in rule(root)]


def main(argv: list[str]) -> int:
    if shutil.which(workflows.YQ) is None:
        print(f"✗ {workflows.YQ} not found. Install via: mise install yq", file=sys.stderr)
        return lib.EXIT_USAGE
    return lib.run(argv, "infra config", lint)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
