# Deploy the Existing Development System

**Owner:** Human
**Executor:** Agent starts the run; Pipeline performs the deployment
**Route:** Routine deployment; use the founding sequence after a wipe

## Readiness

| Order | Executor | Action | Verifier | Required evidence | Blocks next |
|---|---|---|---|---|---|
| 1 | Agent | Confirm this is an existing development installation. | Agent | Steps 01 through 06 of the founding sequence have green evidence. | Yes |
| 2 | Agent | Run the development deployment-input gate. | `02_preflight/00_gate.sh` | Green output with no missing deployment input. | Yes |
| 3 | Agent | Report the exact commit and development target to the Human. Use `main`, or the reviewed candidate procedure below. | Human | The request approving this exact deployment. | Yes |

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

### Reviewed development candidate

When a development demonstration needs an unmerged runtime fix, first finish its local checks, standards review and commits.
Indy approves the exact full commit, feature branch and development API/runner target before branch publication or workflow dispatch.
That approval authorizes only the named development deployment; it grants neither production nor merge authority.

Use the existing `deploy-dev.yml` workflow with the approved branch as `--ref`, keeping the workflow and authentication unchanged.
Record the approval, dispatch time, workflow URL and returned `headSha`; it must equal the approved commit.
A newer commit needs a new exact-revision approval.

Require `check-credentials` and every enabled deployment/verification job to pass, including the runner deployment.
Confirm the API commit stamp maps unambiguously to that revision.

Record active runner identity and compare its installed executable checksum with the workflow's runner binary.
An unavailable runner identity/checksum witness leaves runtime correlation incomplete.

Only then trigger the reviewed demonstration fleet and retain its native event, addressed full tool records and actual answer.
A successful deployment or processed event alone is not a successful demonstration.

### Required pipeline jobs

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

The Agent records the workflow URL, commit, each job result, and any intentionally
skipped runner job. A skipped enabled job is a failure.

`make acceptance-local` retains the full browser and command-line suites.
Deployment uses `make acceptance-e2e ACCEPTANCE_SCOPE=post-deploy` for the short browser check.
`ACCEPTANCE_SCOPE=ten-fleets` selects only ten concurrent fleets and prerequisites.

Indy deferred ten-fleet acceptance to a manual run after merge and development deployment verification on 2026-10-25.
The deployment summary reports `deferred-manual-by-Indy`; it does not report a ten-fleet pass.
The manual workflow keeps its own fixture accounts, assertions, time limits and evidence uploads.

After verifying the development deployment, Indy can start the ten-fleet check:

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
