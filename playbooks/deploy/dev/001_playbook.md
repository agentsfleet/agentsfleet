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
| `compile-runner` and `push-ghcr` | The exact commit built and its image was published. |
| `deploy-fly` | The API and Cloudflare tunnel deployed. |
| `deploy-metal` | The enabled development runner deployed and passed its host gate. |
| `dashboard` | The dashboard and command-line outputs were built, their receipts assembled, and the prebuilt dashboard deployed. Acceptance depends on this job succeeding. |
| `verify` | Public API health and readiness passed. |
| `qa` | Dashboard smoke passed. |
| `acceptance-e2e` | Prerequisites and one real fleet check passed. |
| `acceptance-cli` | The worktree command-line client passed live acceptance. |
| `ten-fleets` | Ten concurrent fleets retained 1,810 messages and separate setup versions against api-dev. |

The Agent records the workflow URL, commit, each job result, and any intentionally
skipped runner job. A skipped enabled job is a failure.

`make acceptance-local` retains the full browser and command-line suites.
Deployment uses `make acceptance-e2e ACCEPTANCE_SCOPE=post-deploy` for the short browser check.
`ACCEPTANCE_SCOPE=ten-fleets` selects only ten concurrent fleets and prerequisites.

The separate ten-fleet job runs after deployment with its own fixture accounts.
Its failure or cancellation keeps the deployment verdict red.

To request another ten-fleet check against the deployed development environment:

```bash
gh workflow run acceptance-ten-fleets.yml --ref main --repo agentsfleet/agentsfleet
```

GitHub returns after queuing the workflow. Inspect the resulting run and its
`acceptance-ten-fleets-results` download for the test verdict and evidence.

## Failure handling

The Agent diagnoses a red job from its logs. A live host mutation, provider
write, or restart of work in flight requires Human approval. The Agent does not
replace a failed Pipeline deployment with an unrecorded workstation deployment.

**There is no shell in the API container.** The image is distroless — the
daemon, a certificate bundle, a clock, and nothing else. `flyctl ssh console`
into an API machine gives no prompt, and that is the image working as intended.
Diagnose from `flyctl logs`, from `/readyz` and `/healthz`, and from the metrics
families; the same rule holds here as in production, and development is where an
operator should discover it.
