---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Verify the Production Installation

## What this does

The Agent reruns the original release, which now deploys the canary and the approved fleet.
Publishing and release creation are idempotent on a rerun.

## Before you begin

Steps [07](../07_deploy_prod/001_playbook.md) and [08](../08_runner_bootstrap_prod/001_playbook.md) are done, and `PROD_RUNNER_READY` is `true`.

## Steps

1. Rerun and watch the release:

   ```bash
   gh run rerun "$PROD_RELEASE_RUN_ID" --repo agentsfleet/agentsfleet
   gh run watch "$PROD_RELEASE_RUN_ID" --repo agentsfleet/agentsfleet --exit-status
   ```

2. After `deploy-metal-canary-prod` is green, the Human approves the `production-fleet` environment.
3. After Human approval, apply the production operational gates:

   ```bash
   ENV=prod ACTION=apply ALLOW_VAULT_READS=1 ALLOW_PROVIDER_WRITES=1 \
     ./playbooks/operations/ip_allowlisting/00_gate.sh
   ENV=prod ACTION=apply ALLOW_VAULT_READS=1 ALLOW_OBSERVABILITY_WRITES=1 \
     ./playbooks/operations/observability/00_gate.sh
   ENV=prod ACTION=verify ALLOW_VAULT_READS=1 \
     ./playbooks/operations/observability/00_gate.sh
   ```

   They end with `PASS: IP allowlisting apply completed for prod`, `PASS: observability apply completed for prod` and `PASS: observability verify completed for prod`.
4. Complete every live-acceptance section the production provider registration playbooks left open.
   Record provider-side identifiers, tenant and fleet identifiers and timestamps, never credentials.

## Verify it works

- Every job in the [production job table](../../deploy/prod/001_playbook.md#verify-it-works) passes; the canary passes before the fleet starts.
- Every runner in `PROD_RUNNER_HOSTS` is active.
- `acceptance-cli-prod` passes against `https://api.agentsfleet.net` with the exact version in `VERSION`.
- npm `latest` is that version:

  ```bash
  test "$(npm view @agentsfleet/cli@latest version)" = "$(cat VERSION)"
  ```

  The command prints nothing and exits 0.
- `https://app.agentsfleet.net` is reachable.

Production is ready only when this step, the provider gates and both public domains are green.
A skipped job is not evidence.
