---
type: how-to
audience: operator
verified: 2026-11-12
product_version: 0.76.0
executable: false
---

# Add or replace a runner

## What this does

Adds a runner host after the founding installation; after a wipe, follow the [founding route](../../README.md#chronological-setup) instead.
Indy creates external state, the agent prepares the host, and the deployment pipeline installs and verifies the runner.

| Environment | Dashboard | 1Password item in the environment's vault |
|---|---|---|
| Development | `https://app-dev.agentsfleet.net` | `agentsfleet-dev-runner-ant` |
| Production | `https://app.agentsfleet.net` | A `vault_key` listed in the `PROD_RUNNER_HOSTS` repository variable |

## Before you begin

1. Indy installs Debian, creates the deploy user and joins Tailscale as `tag:worker`.
2. The agent confirms the expected Tailscale hostname is online.
3. Indy fills `tailscale-hostname` and `deploy-user` in the item.
   Create no placeholder token; deployment refuses an `agt_rFAKE` value.

## Steps

1. After Indy approves, the agent prepares a development host.

   ```bash
   ALLOW_VAULT_READS=1 ALLOW_RUNNER_HOST_PREPARE=1 ./playbooks/founding/05_runner_bootstrap_dev/prepare.sh
   ```

   ```text
   PASS: agentsfleet-dev-runner-ant host preparation completed
   ```

   For production, `<PRODUCTION_RUNNER_ITEM>` is the host's `vault_key`:

   ```bash
   ALLOW_VAULT_READS=1 ALLOW_RUNNER_HOST_PREPARE=1 ENV=prod RUNNER_ITEM=<PRODUCTION_RUNNER_ITEM> ./playbooks/lib/runner/prepare.sh
   ```

   ```text
   PASS: <PRODUCTION_RUNNER_ITEM> host preparation completed
   ```

   Preparation checks the `cpu`, `io`, `memory` and `pids` cgroup v2 controllers, installs packages, enables IPv4 forwarding and creates directories.
   It installs no binary, writes no token and starts no service.
2. The agent enrolls the host with the CLI. The host ID is the item's exact `tailscale-hostname`, and the token lands in a new private file:

   ```bash
   printf '%s' '{"durable_tasks_enabled":true,"sandbox_tier":"landlock_full","network_policy":"allow_all","registry_allowlist":[],"worker_count":4,"extra_binds":[]}' > policy.json
   agentsfleet platform runners enroll --host-id "$(op read "op://$VAULT/<RUNNER_ITEM>/tailscale-hostname")" \
     --policy policy.json --labels dev --token-out runner-token
   ```

   Use `--labels prod` for production.
3. The agent stores the token in the item's `runner-token` field only, through a template file, then deletes both files.
   Use `op item get --format json`, set that one field, `op item edit --template <file>`, and confirm every other field is unchanged.
   If the token is lost, run `agentsfleet platform runners rotate-token`.
4. For production, the agent adds `{"vault_key": "<PRODUCTION_RUNNER_ITEM>"}` to `PROD_RUNNER_HOSTS`.
5. The agent starts the routine deployment: [development](../../deploy/dev/001_playbook.md) or [production](../../deploy/prod/001_playbook.md).
   The pipeline downloads its checked binary, installs the tracked systemd unit, starts the service and runs the host gate; a workstation never deploys it.
6. For production, Indy approves the fleet after the canary is green; the remaining runners deploy one at a time.

## Verify it works

- The item holds a real `runner-token`, not a placeholder.
- The runner deployment and verification jobs are green.
- The host gate confirms `cpu`, `io`, `memory` and `pids` delegation.
- `agentsfleet platform runners list` shows the runner `online`.
