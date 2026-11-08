---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Bootstrap the Production Runners

## What this does

The Human prepares hosts and provider state, and the Agent prepares the hosts.
The Pipeline deploys the runners in step 09; nothing deploys from a workstation.

## Before you begin

[Step 07](../07_deploy_prod/001_playbook.md) is green, and `PROD_RELEASE_RUN_ID` is recorded.

## Steps

### 1. Human and Agent prepare every host

For each host, use the provider console to install Debian 13 and create the deploy user.
Then join the tailnet:

```bash
sudo tailscale up \
  --advertise-tags=tag:worker \
  --hostname "<TAILSCALE_HOSTNAME>" \
  --ssh
```

Store `tailscale-hostname` and `deploy-user` in that host's `ZMB_CD_PROD` item. Create no `runner-token` yet.
Set `PROD_RUNNER_HOSTS` to the inventory; the first entry becomes the canary:

```bash
PROD_RUNNER_HOSTS='[{"vault_key":"runner-1"},{"vault_key":"runner-2"}]'
gh variable set PROD_RUNNER_HOSTS --body "$PROD_RUNNER_HOSTS" --repo agentsfleet/agentsfleet
```

After the Human approves changes to the fresh hosts, the Agent runs:

```bash
ALLOW_VAULT_READS=1 ALLOW_RUNNER_HOST_PREPARE=1 \
  ./playbooks/founding/08_runner_bootstrap_prod/prepare.sh
```

Each host prints `PASS: <VAULT_KEY> host preparation completed`.
Preparation matches [the development step](../05_runner_bootstrap_dev/001_playbook.md#2-agent-prepares-the-host): cgroup check, packages, IPv4 forwarding, directories, no binary or token.

### 2. Human and Agent prime the database

1. Follow [`admin_bootstrap`](../../operations/admin_bootstrap/001_playbook.md) with `ENV=prod`.
2. Work through [Prime a clean database](../../README.md#prime-a-clean-database) steps 3 to 7, with vault names from the production environment. Read each model catalogue diff before approving its write; those rows are billing rates.
3. Leave runner-dependent live acceptance for step 09.
4. Restart `agentsfleetd` by rerunning the release while `PROD_RUNNER_READY` is `false`:

   ```bash
   gh run rerun "$PROD_RELEASE_RUN_ID" --repo agentsfleet/agentsfleet
   gh run watch "$PROD_RELEASE_RUN_ID" --repo agentsfleet/agentsfleet --exit-status
   ```

### 3. Human creates the runner tokens

1. Open `https://app.agentsfleet.net` → **Avatar → Platform → Runners** and add one runner per inventory entry.
2. Set **Host identifier** to that host's `tailscale-hostname` and select **Landlock** under **Isolation**.
3. Store each once-revealed `agt_r` token in the matching item's `runner-token` field.

Never store a placeholder token. If a value is lost, revoke that runner and add it again.

### 4. Agent enables the rollout

```bash
gh variable set PROD_RUNNER_READY --body true --repo agentsfleet/agentsfleet
```

## Verify it works

Every host preparation passed, the inventory is valid JSON and `PROD_RUNNER_READY` is `true`.
Continue to [step 09](../09_verify_prod/001_playbook.md).
