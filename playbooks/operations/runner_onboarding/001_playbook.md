---
type: how-to
audience: operator
verified: 2026-11-12
product_version: 0.76.0
executable: false
---

# Add or replace a runner

## What this does

Adds a runner host after the founding installation; after a wipe, follow the [founding route](../../hosted/README.md#chronological-setup) instead.
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
   It loads the Kernel-based Virtual Machine (KVM) and tap modules, and fails on a host without hardware virtualization.
   It installs no binary, writes no token and starts no service.

   The runner boots a Firecracker virtual machine for each run, so the agent confirms root can open the KVM device:

   ```bash
   tailscale ssh <DEPLOY_USER>@<TAILSCALE_HOSTNAME> 'sudo sh -c "exec 3<>/dev/kvm" && echo usable'
   ```

   ```text
   usable
   ```

   `<DEPLOY_USER>` and `<TAILSCALE_HOSTNAME>` are the item's `deploy-user` and `tailscale-hostname`.
   Any other output means the runner would exit at boot naming `kvm`; fix the host before enrolling it.
2. The agent enrolls the host with the command-line interface (CLI). The host ID is the item's exact `tailscale-hostname`, and the token lands in a new private file.
   `DECISION_MODEL_ID` is the `id` of the `typesafe` row in `agentsfleet platform models list`; a durable task's goal check runs on that model:

   ```bash
   printf '{"durable_tasks_enabled":true,"sandbox_tier":"firecracker","network_policy":"allow_all","registry_allowlist":[],"worker_count":4,"extra_binds":[],"decision_model_id":"%s"}' \
     "$DECISION_MODEL_ID" > policy.json
   agentsfleet platform runners register --host-id "$(op read "op://$VAULT/<RUNNER_ITEM>/tailscale-hostname")" \
     --policy policy.json --labels dev --token-out runner-token
   ```

   Use `--labels prod` for production.
   Under the `firecracker` tier, the daemon marks the runner degraded unless it reports the Firecracker engine.
3. The agent stores the token in the item's `runner-token` field only, through a template file, then deletes both files.
   Use `op item get --format json`, set that one field, `op item edit --template <file>`, and confirm every other field is unchanged.
   If the token is lost, run `agentsfleet platform runners rotate-token <RUNNER_ID> --token-out runner-token`, then store the new file the same way.
4. For production, the agent adds `{"vault_key": "<PRODUCTION_RUNNER_ITEM>"}` to `PROD_RUNNER_HOSTS`.
5. The agent starts the routine deployment: [development](../../deploy/dev/001_playbook.md) or [production](../../deploy/prod/001_playbook.md).
   The pipeline downloads its checked binary, installs the tracked systemd unit, starts the service and runs the host gate; a workstation never deploys it.
6. For production, Indy approves the fleet after the canary is green; the remaining runners deploy one at a time.

## Verify it works

- The item holds a real `runner-token`, not a placeholder.
- The runner deployment and verification jobs are green.
- The host gate confirms `cpu`, `io`, `memory` and `pids` delegation.
- `agentsfleet platform runners list` shows the runner `online`.
- `agentsfleet platform runners show <RUNNER_ID>` shows the engine `firecracker`, the isolation tier `firecracker` and the health `Ready`.
