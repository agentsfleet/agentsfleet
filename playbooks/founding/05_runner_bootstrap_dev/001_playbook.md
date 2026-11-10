---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Bootstrap the Development Runner

## What this does

The Human prepares the host and provider state, the Agent prepares the host, and the Pipeline deploys and verifies the runner.
It ends with the first fully green development run.

## Before you begin

[Step 04](../04_deploy_dev/001_playbook.md) is green and `DEV_RUNNER_READY` is `false`.
The tailnet policy from step 02 lets `tag:ci` reach `tag:worker`.

## Steps

### 1. Human prepares the host

From the provider console, install Debian 13, create the non-root deploy user and install Tailscale.
Join the tailnet:

```bash
sudo tailscale up \
  --advertise-tags=tag:worker \
  --hostname "agentsfleet-dev-runner-ant" \
  --ssh
```

Store `tailscale-hostname` and `deploy-user` in `op://<VAULT_DEV>/agentsfleet-dev-runner-ant`.
Tailscale Secure Shell (SSH) distributes and checks host keys; the automation never disables host-key verification.

### 2. Agent prepares the host

After the Human approves the fresh-host change, run:

```bash
ALLOW_VAULT_READS=1 ALLOW_RUNNER_HOST_PREPARE=1 \
  ./playbooks/founding/05_runner_bootstrap_dev/prepare.sh
```

The last line is `PASS: agentsfleet-dev-runner-ant host preparation completed`.
The script first refuses a host whose cgroup v2 lacks the `cpu`, `io`, `memory` or `pids` controller.
It installs host packages, turns on IPv4 forwarding and creates deployment directories.

The native connection gate requires OpenSSL 3.5 or newer with QUIC and hybrid key exchange, provided by Debian 13.

It installs no runner binary, writes no token and starts no service.

### 3. Human and Agent prime the database

1. Follow [`admin_bootstrap`](../../operations/admin_bootstrap/001_playbook.md) with `ENV=dev`. The Human completes dashboard signup and console actions.
2. Work through [Prime a clean database](../../README.md#prime-a-clean-database) steps 3 to 7, with vault names from the development environment.
3. Leave runner-dependent live acceptance for step 06.
4. Restart with the [development deploy commands](../../deploy/dev/001_playbook.md#steps) while the runner stays disabled.

### 4. Human creates the runner token

1. Open `https://app-dev.agentsfleet.net` → **Avatar → Platform → Runners → Add runner**.
2. Enter the Tailscale hostname as **Host identifier** and select **Landlock** under **Isolation**.
3. Save the once-revealed `agt_r` token straight into the host item's concealed `runner-token` field.

Never store a placeholder token. If the value is lost, revoke the runner and add it again.

### 5. Agent enables the runner and deploys

```bash
gh variable set DEV_RUNNER_READY --body true --repo agentsfleet/agentsfleet
```

Then start and watch a run with the [development deploy commands](../../deploy/dev/001_playbook.md#steps).
Record its `run_id` as `DEV_VERIFY_RUN_ID` for step 06.

In `metal / deploy-metal`, the Pipeline installs the tracked systemd unit and writes the environment file without exposing the token.
It turns on IPv4 forwarding and starts the service.
It fails unless `cpu`, `io`, `memory` and `pids` are delegated to the service and forwarding is on.

## Verify it works

Every job in the [development job table](../../deploy/dev/001_playbook.md#verify-it-works) passes, `deploy-metal` and `qualify-candidate` included.
Continue to [step 06](../06_verify_dev/001_playbook.md).
