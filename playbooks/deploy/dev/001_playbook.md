# Deploy the Existing Development System

**Owner:** Human
**Executor:** Agent starts the run; Pipeline performs the deployment
**Route:** Routine deployment; use the founding sequence after a wipe

## Readiness

| Order | Executor | Action | Verifier | Required evidence | Blocks next |
|---|---|---|---|---|---|
| 1 | Agent | Confirm this is an existing development installation. | Agent | Steps 01 through 06 of the founding sequence have green evidence. | Yes |
| 2 | Agent | Run the development deployment-input gate. | `02_preflight/00_gate.sh` | Green output with no missing deployment input. | Yes |
| 3 | Agent | Report the exact `main` commit and deployment target to the Human. | Human | The request to start this deployment. | Yes |

Run the read-only gate:

```bash
ENV=dev STAGE=deployment \
  ./playbooks/founding/02_preflight/00_gate.sh
```

If the installation or its evidence is absent, stop and route to
[`founding/README.md`](../../founding/README.md). Do not infer a rebuild.

## Deploy

The Human's request to start this page authorizes the Agent to trigger the
development workflow. External provider writes still require separate approval.

```bash
gh workflow run deploy-dev.yml \
  --ref main \
  --repo agentsfleet/agentsfleet

run_id="$(
  gh run list \
    --repo agentsfleet/agentsfleet \
    --workflow deploy-dev.yml \
    --branch main \
    --limit 1 \
    --json databaseId \
    --jq '.[0].databaseId'
)"
gh run watch "$run_id" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

## Verification

| Pipeline job | What it proves |
|---|---|
| `check-credentials` | Current development deployment inputs exist. |
| `compile-dev` and `push-dev` | The exact commit built and its image was published. |
| `deploy-fly-dev` | The API and Cloudflare tunnel deployed. |
| `deploy-worker-dev` | The enabled development runner deployed and passed its host gate. |
| `verify-dev` | Public API health and readiness passed. |
| `qa-dev` | Dashboard smoke passed. |
| `acceptance-e2e-dev` | Browser acceptance passed. |
| `cli-acceptance-dev` | The worktree command-line client passed live acceptance. |

The Agent records the workflow URL, commit, each job result, and any intentionally
skipped runner job. A skipped enabled job is a failure.

## Failure handling

The Agent diagnoses a red job from its logs. A live host mutation, provider
write, or restart of work in flight requires Human approval. The Agent does not
replace a failed Pipeline deployment with an unrecorded workstation deployment.
