# Deploy the Existing Production System

**Owner:** Human
**Executor:** Agent starts the release; Pipeline performs the deployment
**Route:** Routine deployment; use the founding sequence after a wipe

## Readiness

| Order | Executor | Action | Verifier | Required evidence | Blocks next |
|---|---|---|---|---|---|
| 1 | Agent | Confirm this is an existing production installation. | Agent | Founding step 09 has green evidence. | Yes |
| 2 | Agent | Run the production deployment-input gate. | `02_preflight/00_gate.sh` | Green output with no missing deployment input. | Yes |
| 3 | Agent | Verify development health and production domains. | HTTP checks | Development readiness plus production API and dashboard routes resolve. | Yes |
| 4 | Agent | Report the exact version, commit, tag, and targets. | Human | Explicit approval for that release tag. | Yes |

Run only read-only checks before requesting approval:

```bash
ENV=prod STAGE=deployment \
  ./playbooks/founding/02_preflight/00_gate.sh

curl -fsS https://api-dev.agentsfleet.net/readyz \
  | jq -e '.ready == true'
curl -fsS https://api.agentsfleet.net/readyz \
  | jq -e '.ready == true'
curl -fsS -o /dev/null https://app.agentsfleet.net

release_tag="v$(cat VERSION)"
git rev-parse HEAD
printf 'release tag: %s\n' "$release_tag"
```

If the production installation or step 09 evidence is absent, stop and route to
[`founding/README.md`](../../founding/README.md). If a public domain is absent,
report it as a Human-owned blocker and do not create the tag.

## Human approval: release

The Human approves the exact version and tag reported above. Only then may the
Agent run:

```bash
git tag "$release_tag"
git push origin "$release_tag"

run_id="$(
  gh run list \
    --repo agentsfleet/agentsfleet \
    --workflow release.yml \
    --limit 20 \
    --json databaseId,headBranch \
    --jq ".[] | select(.headBranch == \"$release_tag\") | .databaseId" \
    | head -1
)"
gh run watch "$run_id" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

## Human approval: runner fleet

The Pipeline deploys and verifies the first production runner. The Agent reports
the canary job URL and evidence. The Human then approves the
`production-fleet` GitHub environment. No remaining runner starts before that
approval.

## Verification

| Pipeline job | What it proves |
|---|---|
| `check-credentials` and `verify-tag` | Inputs exist and the tag matches `VERSION`. |
| Release build and publication jobs | Binaries, image, npm `next`, and the GitHub release use the approved version. |
| `verify-dev-gate` | Development remains ready before production changes. |
| `deploy-fly-prod` | Production API, tunnel, health, and readiness passed. |
| `deploy-worker-canary-prod` | The first runner deployed and passed its host gate. |
| `deploy-worker-fleet-prod` | The Human-approved remaining fleet deployed sequentially. |
| Post-release verification | Exact-version installs and live command-line acceptance passed before npm `latest` moved. |

After the Pipeline is green, the Agent runs each configured provider's read-only
verification adapter and records its result. Production is eligible only when
the workflow, provider checks, public domains, and package verification are all
green. A skipped check is not evidence.

## Failure handling

The Agent diagnoses a red job from its logs. A provider write, live host change,
release replacement, or restart of work in flight requires new Human approval.

**There is no shell in the API container.** The image is distroless: it carries
the daemon, a certificate bundle and a clock, and nothing else — no shell, no
package manager, no `wget`. `flyctl ssh console` into an API machine will not
give a prompt, and that is the image working as intended rather than a broken
deploy. Diagnose from what the daemon publishes instead: `flyctl logs` for the
logfmt stream, `/readyz` and `/healthz` over the tunnel for liveness, and the
metrics families for behaviour. A question none of those can answer is a
question the daemon should be reporting and currently is not — the fix is an
emit, not a shell.
