---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Deploy the Development Control Plane

## What this does

The first development deployment after a wipe brings up the API, tunnel and dashboard with the runner disabled.

## Before you begin

[Step 03](../03_priming_infra/001_playbook.md) is green.
The `main` tree has a successful `test.yml` run; `deploy-dev.yml` promotes its build outputs and fails without one.

## Steps

1. Disable the runner before any deploy, because every push to `main` starts `deploy-dev.yml`:

   ```bash
   gh variable set DEV_RUNNER_READY --body false --repo agentsfleet/agentsfleet
   gh variable get DEV_RUNNER_READY --repo agentsfleet/agentsfleet
   ```

   The second command prints `false`.
2. Start and watch the run with the [development deploy commands](../../deploy/dev/001_playbook.md#steps).
3. If `https://app-dev.agentsfleet.net` does not reach the development Vercel deployment, the Human attaches the domain in Vercel and repairs DNS.
4. Record the run URL and identifier.

## Verify it works

- `check-credentials`, `build / promote`, `fly / deploy-fly`, `dashboard / dashboard` and `verify / verify` pass.
- `metal / deploy-metal` and `qualify-candidate` skip because `DEV_RUNNER_READY` is `false`.
- `acceptance / qa`, `acceptance-e2e` and `acceptance-cli` still run; record their results. Full acceptance is required at step 05.
- `https://api-dev.agentsfleet.net/readyz` succeeds through the tunnel, and Fly's private `/readyz` check passes.
- `https://app-dev.agentsfleet.net` resolves to the development Vercel deployment.

The model catalogue ships empty, so the environment is not usable yet.
[Step 05](../05_runner_bootstrap_dev/001_playbook.md) primes it; keep the runner disabled until then.
