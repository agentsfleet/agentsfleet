---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Deploy the Production System

## What this does

Pushing a `v*` tag starts `release.yml`.
The release builds nothing: it promotes the development-qualified candidate for the tagged source tree.
That candidate exists only after a `deploy-dev.yml` run on the same tree where every gate, runner included, passed.

When `Release` succeeds, `post-release.yml` verifies installs, runs live command-line acceptance and moves npm `latest`.
The founding steps use this page's [release commands](#steps) too.

## Before you begin

1. Confirm founding step 09 has green evidence. If not, stop and use [chronological setup](../../README.md#chronological-setup).
2. Run only read-only checks:

   ```bash
   ENV=prod STAGE=deployment ./playbooks/founding/02_preflight/00_gate.sh
   curl -fsS https://api-dev.agentsfleet.net/readyz | jq -e '.ready == true'
   curl -fsS https://api.agentsfleet.net/readyz | jq -e '.ready == true'
   curl -fsS -o /dev/null https://app.agentsfleet.net
   release_tag="v$(cat VERSION)"
   git rev-parse HEAD
   printf 'release tag: %s\n' "$release_tag"
   ```

   The gate ends with `✅ 002_preflight check complete (env: prod, stage: deployment)`.
   Each `jq -e` prints `true`. The script ends with `<COMMIT_SHA>` and `release tag: v<VERSION>`.
3. A missing public domain is a Human-owned blocker. Report it and create no tag.
4. Report the version, commit, tag and targets. The Human approves that exact tag.

## Steps

After the Human approves the tag, the Agent runs:

```bash
git tag "$release_tag"
git push origin "$release_tag"
run_id="$(gh run list --repo agentsfleet/agentsfleet --workflow release.yml --limit 20 \
  --json databaseId,headBranch \
  --jq ".[] | select(.headBranch == \"$release_tag\") | .databaseId" | head -1)"
gh run watch "$run_id" --repo agentsfleet/agentsfleet --exit-status
```

`gh run watch` exits 0 only when the run succeeds.

When `PROD_RUNNER_READY == 'true'`, the Pipeline deploys and verifies the first runner in `PROD_RUNNER_HOSTS`.
The Agent reports the canary job URL.

The Human then approves the `production-fleet` GitHub environment. No other runner starts before that approval.

## Verify it works

| Job | Proves |
|---|---|
| `check-credentials`, `verify-tag` | Inputs exist; the tag equals `v` plus `VERSION` |
| `prepare-artifacts` | A development-qualified candidate for this tree exists and its signatures verify |
| `push-ghcr`, `npm`, `create-release` | Image promoted, npm `next` published, GitHub Release created |
| `verify-dev-gate` | Development `/healthz` and `/readyz` pass before production changes |
| `deploy-fly-prod` | Collector, NATS cluster, API (three machines), tunnel and readiness pass |
| `deploy-metal-canary-prod` | The first runner deployed and passed its host gate |
| `deploy-metal-fleet-prod` | The approved remaining runners deployed one by one |
| `post-release.yml`: `verify-npm` | The exact version installs on macOS, Linux and Windows |
| `post-release.yml`: `acceptance-cli-prod`, `promote-latest` | Live command-line acceptance passed, then npm `latest` moved |

The canary, fleet and `acceptance-cli-prod` jobs skip while `PROD_RUNNER_READY` is not `true`; `promote-latest` then skips too.
After green runs, the Agent runs each provider's read-only verification and records the result.
Production is eligible only when the workflows, provider checks, public domains and package checks are all green.

A skipped check is not evidence.

## Common problems

The Agent diagnoses a red job from its logs.
A provider write, live host change, release replacement or restart of work in flight needs new Human approval.
The API container has no shell; [`ARCHITECTURE.md`](../../ARCHITECTURE.md#limits) says how to diagnose it.
