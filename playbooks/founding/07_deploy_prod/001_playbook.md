---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Deploy the Production Control Plane

## What this does

The first production release after a wipe deploys the API and publishes the command-line package under npm `next`.
Runner deployment, live command-line acceptance and npm `latest` stay closed until step 09.

## Before you begin

[Step 06](../06_verify_dev/001_playbook.md) is fully green.
The tag's source tree must equal the tree of the green step 05 run; that run's `qualify-candidate` is what `release.yml` promotes.

## Steps

1. Check production inputs and disable production runners:

   ```bash
   ENV=prod STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh
   gh variable set PROD_RUNNER_READY --body false --repo agentsfleet/agentsfleet
   ```

   The gate ends with `✅ 002_preflight check complete (env: prod, stage: deployment)`.
2. Set `release_tag="v$(cat VERSION)"`.
3. The Human approves the exact version and tag `v$(cat VERSION)`. Record the approval verbatim.
4. Push the tag and watch the run with the [production release commands](../../deploy/prod/001_playbook.md#steps).
5. If `https://app.agentsfleet.net` does not reach the production Vercel deployment, the Human attaches the domain.
6. Record the tag, commit, run identifier as `PROD_RELEASE_RUN_ID` and every job result.

## Verify it works

- Every job in the [production job table](../../deploy/prod/001_playbook.md#verify-it-works) through `deploy-fly-prod` passes.
- `deploy-metal-canary-prod`, `deploy-metal-fleet-prod`, `acceptance-cli-prod` and `promote-latest` skip because `PROD_RUNNER_READY` is `false`.
- `verify-npm` passes for the exact version in `VERSION`.
- `https://api.agentsfleet.net/readyz` passes through Cloudflare, and Fly's private `/readyz` check passes.

The model catalogue ships empty; [step 08](../08_runner_bootstrap_prod/001_playbook.md) primes it.
