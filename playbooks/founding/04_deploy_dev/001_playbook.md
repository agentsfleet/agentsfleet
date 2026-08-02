# Deploy the Development Control Plane

**Owner:** Human
**Executors:** Agent starts the run; Pipeline performs the deployment
**Prerequisite:** `playbooks/founding/03_priming_infra/001_playbook.md`

This is the first development deployment after a wipe. It brings up the API and
dashboard while runner-dependent acceptance stays closed.

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Agent | Keep runner deployment disabled. | GitHub variable query | `DEV_WORKER_READY=false`. |
| 2 | Agent | Start and watch the development deployment. | Pipeline | Green workflow URL. |
| 3 | Human | Repair Vercel or DNS ownership if the dashboard domain is absent. | Agent | Dashboard URL resolves to the intended Vercel project. |
| 4 | Agent | Record the successful workflow run identifier. | Agent | Identifier and URL retained for the next step. |

## Run

```bash
gh variable set DEV_WORKER_READY \
  --body "false" \
  --repo agentsfleet/agentsfleet

gh workflow run deploy-dev.yml \
  --ref main \
  --repo agentsfleet/agentsfleet

DEV_DEPLOY_RUN_ID="$(
  gh run list \
    --repo agentsfleet/agentsfleet \
    --workflow deploy-dev.yml \
    --branch main \
    --limit 1 \
    --json databaseId \
    --jq '.[0].databaseId'
)"
gh run watch "$DEV_DEPLOY_RUN_ID" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

Keep `DEV_DEPLOY_RUN_ID`; step 05 downloads its runner artifact.

## Required result

- Credential, compile, image, Fly.io, tunnel, API readiness, and dashboard smoke
  jobs pass.
- `deploy-worker-dev`, browser acceptance, and Command-Line Interface (CLI)
  acceptance skip because `DEV_WORKER_READY=false`.
- `https://api-dev.agentsfleet.net/readyz` responds successfully through the
  Cloudflare tunnel. Fly.io also checks `/readyz` privately on the
  `agentsfleetd` machine.
- `https://app-dev.agentsfleet.net` resolves to the development Vercel
  deployment. If it does not, the Human attaches the domain in Vercel and repairs
  DNS before step 05.

Do not enable the runner yet. Continue to
`playbooks/founding/05_runner_bootstrap_dev/001_playbook.md`.
