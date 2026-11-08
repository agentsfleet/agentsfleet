---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Deploy the Development System

## What this does

`deploy-dev.yml` deploys the development API, tunnel, runner and dashboard, then runs acceptance.
Every push to `main` starts it. `workflow_dispatch` redeploys a chosen ref through the same checks.

The founding steps use this page's [run commands](#steps) too.

The workflow builds nothing.
It promotes the build outputs of a successful `test.yml` run for the exact same source tree, and fails when none exists.

## Before you begin

1. Confirm founding steps 01 through 06 have green evidence. If not, stop and use [chronological setup](../../README.md#chronological-setup); never infer a rebuild.
2. Run the read-only input gate:

   ```bash
   ENV=dev STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh
   ```

   The last line is `✅ 002_preflight check complete (env: dev, stage: deployment)`.
3. Report the exact commit and target to the Human. The Human's request to start this page authorizes the dispatch. A provider write still needs its own approval.

> **Warning:** After a database wipe, set `DEV_RUNNER_READY` to `false` before the next deploy, including a push to `main`.
> [Founding step 05](../../founding/05_runner_bootstrap_dev/001_playbook.md) sets it back to `true` after the new runner token is stored.

## Steps

```bash
gh workflow run deploy-dev.yml --ref main --repo agentsfleet/agentsfleet
run_id="$(gh run list --repo agentsfleet/agentsfleet --workflow deploy-dev.yml \
  --branch main --limit 1 --json databaseId --jq '.[0].databaseId')"
gh run watch "$run_id" --repo agentsfleet/agentsfleet --exit-status
```

`gh run watch` exits 0 only when the run succeeds.

## Verify it works

| Job | Proves |
|---|---|
| `check-credentials` | Development deployment inputs exist |
| `build / promote` | The `test.yml` candidate for this tree verified; toolbox scanned and signed; image published |
| `fly / deploy-fly` | OTLP collector, Dragonfly cluster and API deployed |
| `metal / deploy-metal` | Runner deployed and passed its host gate; runs only when `DEV_RUNNER_READY == 'true'` |
| `dashboard / dashboard` | Candidate command-line and dashboard builds made; prebuilt dashboard deployed; acceptance needs it |
| `verify / verify` | Tunnel running; public `/healthz` and `/readyz` pass |
| `acceptance / qa` | Dashboard smoke passed |
| `acceptance / acceptance-e2e` | `make acceptance-e2e ACCEPTANCE_SCOPE=post-deploy`: prerequisites and one real fleet |
| `acceptance / acceptance-cli` | `make acceptance-cli`: the command-line client built from this commit, against `api-dev` |
| `qualify-candidate` | Every gate, runner included, succeeded; uploads `development-candidate-<TREE>`, which `release.yml` requires |

Record the workflow URL, commit, every job result and any intended `deploy-metal` skip.
Any other skipped job is a failure.

`make acceptance-local` keeps the full browser and command-line suites.
`ACCEPTANCE_SCOPE=ten-fleets` runs only ten concurrent fleets and their prerequisites.
On 2026-10-25, Indy deferred ten-fleet acceptance to a manual run after a verified development deploy.

The deploy summary reports `deferred-manual-by-Indy`, never a ten-fleet pass.
Indy starts it with:

```bash
gh workflow run acceptance-ten-fleets.yml --ref main --repo agentsfleet/agentsfleet
```

The command returns once GitHub queues the run.
Read the verdict and evidence in that run's `acceptance-ten-fleets-results` download.

## Deploy an unmerged development candidate

Use this only when a development demonstration needs an unmerged runtime fix.

1. Finish the fix's local checks, standards review and commits.
2. Indy approves the exact full commit, feature branch and development API and runner target. That approval covers only this deployment: no production, no merge.
3. Dispatch `deploy-dev.yml` with the approved branch as `--ref`. Change neither the workflow nor its authentication.
4. Record the approval, dispatch time, workflow URL and returned `headSha`. `headSha` must equal the approved commit; a newer commit needs a new approval.
5. Require `check-credentials` and every enabled job to pass, runner deploy included. Confirm the API commit stamp maps to that revision alone.
6. Record the active runner identity. Compare its installed executable checksum with the workflow's runner binary. Without that witness, runtime correlation is incomplete.
7. Only then trigger the reviewed demonstration fleet. Keep its native event, full addressed tool records and actual answer. A deployment or a processed event alone is not a successful demonstration.

## Common problems

The Agent diagnoses a red job from its logs.
A live host change, provider write or restart of work in flight needs Human approval.
Never replace a failed Pipeline deploy with an unrecorded workstation deploy.

The API container has no shell; [`ARCHITECTURE.md`](../../ARCHITECTURE.md#limits) says how to diagnose it.
