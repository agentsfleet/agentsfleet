---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Verify the Development Installation

## What this does

The Agent records the step 05 run and applies the development operational gates.
Start no new deployment here.

## Before you begin

[Step 05](../05_runner_bootstrap_dev/001_playbook.md) is done, `DEV_RUNNER_READY` is `true` and the `DEV_VERIFY_RUN_ID` run is green.

## Steps

1. Confirm the run's API readiness, dashboard smoke, runner deploy, browser acceptance and command-line acceptance all passed.
2. After Human approval, apply allowlisting and observability:

   ```bash
   ENV=dev ACTION=apply ALLOW_VAULT_READS=1 ALLOW_PROVIDER_WRITES=1 \
     ./playbooks/operations/ip_allowlisting/00_gate.sh
   ENV=dev ACTION=apply ALLOW_VAULT_READS=1 ALLOW_OBSERVABILITY_WRITES=1 \
     ./playbooks/operations/observability/00_gate.sh
   ENV=dev ACTION=verify ALLOW_VAULT_READS=1 \
     ./playbooks/operations/observability/00_gate.sh
   ```

   The three commands end with `PASS: IP allowlisting apply completed for dev`, `PASS: observability apply completed for dev` and `PASS: observability verify completed for dev`.
   It needs no Human dashboard confirmation; PlanetScale is the only provider, and its API reports its own restrictions.
3. Complete every live-acceptance section the provider registration playbooks left open.

## Verify it works

Record the workflow URL, every job result, each gate result and provider-side identifiers: tenant, fleet and timestamps.
Record no credential value.
Production waits while any job or gate is red.

Continue to [step 07](../07_deploy_prod/001_playbook.md).
