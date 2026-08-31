# Deploy the Production Control Plane

**Owner:** Human
**Executors:** Agent starts the approved release; Pipeline deploys and verifies
**Prerequisite:** step 06 is fully green

The first production release after a wipe deploys the API and publishes the
command-line interface (CLI) package under the `next` npm tag. Worker
deployment, live CLI acceptance, and promotion to `latest` stay closed until
step 08.

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Agent | Verify deployment inputs and disable production runners. | Preflight and GitHub variable query | Green gate and `PROD_RUNNER_READY=false`. |
| 2 | Human | Confirm the exact version and authorize its release tag. | Agent | Verbatim approval tied to the tag. |
| 3 | Agent | Push the approved tag and watch the release workflow. | Pipeline | Green release workflow URL. |
| 4 | Human | Attach `app.agentsfleet.net` in Vercel if absent. | Agent | Dashboard URL resolves to the production project. |
| 5 | Agent | Record release evidence. | Agent | Tag, commit, workflow identifier, and job results recorded. |

## Run

```bash
ENV=prod STAGE=deployment \
  ./playbooks/founding/02_preflight/00_gate.sh

gh variable set PROD_RUNNER_READY \
  --body "false" \
  --repo agentsfleet/agentsfleet

RELEASE_TAG="v$(cat VERSION)"
git tag "$RELEASE_TAG"
git push origin "$RELEASE_TAG"

PROD_RELEASE_RUN_ID="$(
  gh run list \
    --repo agentsfleet/agentsfleet \
    --workflow release.yml \
    --limit 1 \
    --json databaseId,headBranch \
    --jq ".[] | select(.headBranch == \"$RELEASE_TAG\") | .databaseId" |
    head -1
)"
gh run watch "$PROD_RELEASE_RUN_ID" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

## Required result

- Binaries, runtime compatibility, image, npm `next`, and GitHub Release jobs
  pass.
- The development readiness check passes before production deployment.
- Production API, tunnel, health, and readiness checks pass.
- `core.model_library` is populated. It ships EMPTY and every fleet needs a
  model, so a green release is not yet a usable environment. Prime it with
  `playbooks/operations/model_catalogue/001_playbook.md`, reading the diff
  before approving the write — these rows are billing rates:

  ```bash
  ACTION=diff  ENV=prod ALLOW_VAULT_READS=1 \
    ./playbooks/operations/model_catalogue/00_gate.sh
  ACTION=apply ENV=prod ALLOW_VAULT_READS=1 ALLOW_MODEL_CATALOGUE_WRITES=1 \
    ./playbooks/operations/model_catalogue/00_gate.sh
  ```
- Production runner jobs skip because `PROD_RUNNER_READY=false`.
- Post-release installation checks pass against the exact package version in
  `VERSION`; live CLI acceptance and `latest` promotion skip.
- `https://app.agentsfleet.net` resolves to the production Vercel deployment.
  If it does not, the Human attaches the custom domain before step 08.
- `https://api.agentsfleet.net/readyz` passes through Cloudflare, and Fly.io's
  private machine check passes the same path internally.

Keep `RELEASE_TAG` and `PROD_RELEASE_RUN_ID`. Continue to
`playbooks/founding/08_runner_bootstrap_prod/001_playbook.md`.
