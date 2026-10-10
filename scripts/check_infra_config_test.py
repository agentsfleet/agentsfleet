#!/usr/bin/env python3
"""Each infrastructure rule refuses the configuration drift it was written for.

    python3 scripts/check_infra_config_test.py
"""

from __future__ import annotations

import contextlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True

import check_infra_config as checker

UNIT = "[Unit]\nDescription=runner\n\n[Service]\nRestart=always\nRestartSec=5\nTimeoutStopSec=300\nExecStart=/usr/local/bin/agentsfleet-runner\n"
DEPLOY = "#!/usr/bin/env bash\nreadonly DRAIN_TIMEOUT_SECONDS=300\nreadonly HEALTH_STABLE_SECONDS=30\n"

POLICY = """\
// The canonical tailnet policy.
{
  "grants": [
    {"src": ["autogroup:member", "tag:ci"], "dst": ["tag:worker"], "ip": ["tcp:22"]}, // members and CI reach SSH
  ],
  /* CI reaches workers as a non-root user. */
  "ssh": [
    {"action": "accept", "src": ["tag:ci"], "dst": ["tag:worker"], "users": ["autogroup:nonroot"]},
  ],
  "sshTests": [
    {"src": "tag:ci", "dst": ["tag:worker"], "accept": ["debian"], "deny": ["root"]},
  ],
  "tagOwners": {"tag:ci": ["autogroup:admin"], "tag:worker": ["autogroup:admin", "https://not-a-comment.example"]},
}
"""

BOOTSTRAP = """\
# Bootstrap

Never join with `--advertise-tags=tag:ci`; that undoes the tag split.

```bash
sudo tailscale up \\
  --auth-key "<one-time-key>" \\
  --advertise-tags=tag:worker \\
  --ssh
```
"""


# Each environment's API pool, sized for its replica count.
POOLS = {"dev": "8", "prod": "5"}


def api_toml(environment: str) -> str:
    pool = POOLS[environment]
    return f"""\
app = "agentsfleetd-{environment}"
[env]
  DATABASE_POOL_SIZE_API = "{pool}"
[[restart]]
  policy = "always"
[checks.readiness]
  type = "http"
  port = 3000
  path = "/readyz"
"""


def tunnel_toml(environment: str) -> str:
    return f'app = "cloudflared-{environment}"\n[[restart]]\n  policy = "always"\n'


def tunnel_config(environment: str) -> str:
    return (
        f"ingress:\n  - hostname: {checker.HOSTNAMES[environment]}\n"
        f"    service: http://agentsfleetd-{environment}.internal:3000\n  - service: http_status:404\n"
    )


RELEASE = """\
on: push
jobs:
  scale:
    runs-on: ubuntu-latest
    steps:
      - run: |
          DESIRED_API_MACHINES=3
          flyctl scale count "$DESIRED_API_MACHINES" --app agentsfleetd-prod
          test "$TOTAL" -eq "$DESIRED_API_MACHINES"
          test "$RUNNING" -eq "$DESIRED_API_MACHINES"
"""


def clean_tree() -> dict[str, str]:
    files = {
        checker.RUNNER_UNIT: UNIT,
        checker.DEPLOY_SH: DEPLOY,
        checker.TAILNET_POLICY: POLICY,
        f"{checker.PLAYBOOKS}/founding/05_bootstrap/001_playbook.md": BOOTSTRAP,
        checker.RELEASE: RELEASE,
    }
    for environment in checker.HOSTNAMES:
        files[f"{checker.FLY_DIR}/agentsfleetd-{environment}/fly.toml"] = api_toml(environment)
        files[f"{checker.FLY_DIR}/cloudflared-{environment}/fly.toml"] = tunnel_toml(environment)
        files[f"{checker.FLY_DIR}/cloudflared-{environment}/config.yml"] = tunnel_config(environment)
    return files


def lint_tree(files: dict[str, str]) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for relative_path, text in files.items():
            (root / relative_path).parent.mkdir(parents=True, exist_ok=True)
            (root / relative_path).write_text(text, encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = checker.main([checker.lib.ROOT_FLAG, str(root)])
        return code, out.getvalue()


def seeded(path: str, old: str, new: str) -> dict[str, str]:
    files = clean_tree()
    assert old in files[path], (path, old)
    files[path] = files[path].replace(old, new)
    return files


PROD_API = f"{checker.FLY_DIR}/agentsfleetd-prod/fly.toml"
DEV_TUNNEL = f"{checker.FLY_DIR}/cloudflared-dev/fly.toml"
BOOTSTRAP_PATH = f"{checker.PLAYBOOKS}/founding/05_bootstrap/001_playbook.md"


class MovedRulesRejectSeededViolations(unittest.TestCase):
    def test_the_clean_fixture_passes(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_moved_rules_reject_seeded_violations(self) -> None:
        cases = {
            "infra-restart-window": seeded(checker.RUNNER_UNIT, "RestartSec=5", "RestartSec=1min"),
            "infra-drain-timeout": seeded(checker.DEPLOY_SH, "DRAIN_TIMEOUT_SECONDS=300", "DRAIN_TIMEOUT_SECONDS=120"),
            "infra-tailnet-policy": seeded(checker.TAILNET_POLICY, '"grants": [', '"grants": [[['),
            "infra-tailnet-tag-owners": seeded(checker.TAILNET_POLICY, '"tag:ci": ["autogroup:admin"]', '"tag:ci": []'),
            "infra-tailnet-ci-ssh": seeded(checker.TAILNET_POLICY, '"users": ["autogroup:nonroot"]', '"users": ["root"]'),
            "infra-tailnet-grant": seeded(checker.TAILNET_POLICY, '"ip": ["tcp:22"]', '"ip": ["*"]'),
            "infra-tailnet-sshtests": seeded(checker.TAILNET_POLICY, '"dst": ["tag:worker"], "accept"', '"dst": ["agentsfleet-dev-runner-ant"], "accept"'),
            "infra-worker-tag": seeded(BOOTSTRAP_PATH, "--advertise-tags=tag:worker", "--advertise-tags=tag:ci"),
            "infra-fly-app": seeded(DEV_TUNNEL, 'app = "cloudflared-dev"', 'app = "cloudflared"'),
            "infra-fly-restart": seeded(DEV_TUNNEL, 'policy = "always"', 'policy = "on-failure"'),
            "infra-fly-private": seeded(PROD_API, "[[restart]]", "[http_service]\n  internal_port = 3000\n[[restart]]"),
            "infra-fly-readiness": seeded(PROD_API, 'path = "/readyz"', 'path = "/healthz"'),
            "infra-tunnel-ingress": seeded(f"{checker.FLY_DIR}/cloudflared-prod/config.yml", "  - service: http_status:404\n", ""),
            "infra-pool-budget": seeded(PROD_API, 'DATABASE_POOL_SIZE_API = "5"', 'DATABASE_POOL_SIZE_API = "6"'),
            "infra-fly-scale": seeded(checker.RELEASE, 'test "$RUNNING" -eq', 'test "$RUNNING" -ge'),
        }
        for rule, files in cases.items():
            with self.subTest(rule=rule):
                code, output = lint_tree(files)
                self.assertEqual(checker.lib.EXIT_FINDINGS, code, output)
                self.assertIn(f": {rule}: ", output)

    def test_a_restart_delay_in_milliseconds_is_read_as_systemd_reads_it(self) -> None:
        self.assertEqual(0.5, checker.timespan_seconds("500ms"))
        self.assertEqual(90, checker.timespan_seconds("1min 30s"))
        self.assertIsNone(checker.timespan_seconds("5 fortnights"))
        code, output = lint_tree(seeded(checker.RUNNER_UNIT, "RestartSec=5", "RestartSec=500ms"))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)

    def test_a_stop_timeout_in_minutes_is_the_same_drain(self) -> None:
        code, output = lint_tree(seeded(checker.RUNNER_UNIT, "TimeoutStopSec=300", "TimeoutStopSec=5min"))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)
        code, output = lint_tree(seeded(checker.RUNNER_UNIT, "TimeoutStopSec=300", "TimeoutStopSec=10min"))
        self.assertIn(f"{checker.DEPLOY_SH}:1: infra-drain-timeout: ", output)

    def test_the_replica_count_comes_from_the_release_workflow(self) -> None:
        code, output = lint_tree(seeded(checker.RELEASE, "DESIRED_API_MACHINES=3", "DESIRED_API_MACHINES=2"))
        self.assertEqual(checker.lib.EXIT_CLEAN, code, output)
        code, output = lint_tree(seeded(checker.RELEASE, "DESIRED_API_MACHINES=3", "DESIRED_API_MACHINES=4"))
        self.assertIn(f"{PROD_API}:1: infra-pool-budget: ", output)

    def test_a_prose_warning_outside_a_code_block_is_not_a_command(self) -> None:
        code, output = lint_tree(clean_tree())
        self.assertNotIn("infra-worker-tag", output)


if __name__ == "__main__":
    unittest.main()
