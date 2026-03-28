# M3_002: Playbook — Deploy PROD

**Milestone:** M3
**Workstream:** 002
**Updated:** Mar 20, 2026
**Owner:** Agent
**Prerequisite:** `playbooks/M3_001_DEPLOY_DEV.md` completed with evidence

This is the canonical step-by-step PROD deployment runbook.

---

## 1.0 Preflight Gate

1. Validate PROD credentials:

```bash
ENV=prod ./playbooks/gates/check-credentials.sh
```

2. Confirm DEV is green (M3_001 complete with CLI acceptance evidence).
3. Confirm `VERSION` file matches intended release tag:
```bash
cat VERSION   # must match the tag you're about to push, e.g. 0.2.0
```

4. Confirm `CHANGELOG.md` has a `## [X.Y.Z]` section for this version.

5. Confirm `deploy.sh` is bootstrapped and tested on all worker nodes (M2_002 §4.7). **Do not cut a release tag before this is verified.** If not done:
```bash
for node in zombie-prod-worker-ant zombie-prod-worker-bird; do
  KEY=$(op read "op://$VAULT_PROD/$node/ssh-private-key")
  ssh -i <(echo "$KEY") "$node" "ls -la /opt/zombie/deploy.sh"
done
```

6. Confirm Fly API token is in vault: `op read "op://$VAULT_PROD/fly-api-token/credential"` (non-empty).

---

## 2.0 Trigger Release + PROD Deploy

1. Create release tag and push:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

2. Confirm `.github/workflows/release.yml` starts.

Expected order:

1. `verify-tag`
2. `binaries`
3. `docker`
4. `npm`
5. `create-release`
6. `verify-dev-gate`
7. `deploy-prod-api` — Fly.io API deploy + healthz/readyz verification
8. `deploy-prod-canary` — first worker host from `PROD_WORKER_HOSTS` (drain + deploy)
9. `deploy-prod-fleet` — remaining hosts (requires `production-fleet` environment approval)

---

## 3.0 Deploy-Prod Behavior

The release pipeline deploys in three stages:

### 3.1 `deploy-prod-api` — Fly.io API

1. Load secrets from 1Password (Fly token, DB URLs, Redis, etc.)
2. Sync secrets to Fly.io, deploy image, verify `/healthz` + `/readyz`

### 3.2 `deploy-prod-canary` — First Worker Host

1. Load tailscale authkey + Discord webhook from 1Password
2. Parse the first entry from `PROD_WORKER_HOSTS` GitHub variable
3. Load that host's SSH key dynamically from vault (`op://$VAULT_PROD/<vault_key>/ssh-private-key`)
4. Join tailnet, SSH to canary host, run `deploy.sh executor` then `deploy.sh worker`
5. `deploy.sh` now **drains the worker gracefully** before restart: sends SIGTERM, watches journalctl for `worker.drain_complete` or `worker.drain_timeout` log lines (up to 300s), then restarts

### 3.3 `deploy-prod-fleet` — Remaining Hosts (Approval Gate)

1. Requires manual approval via the `production-fleet` GitHub environment
2. Loops sequentially over remaining hosts in `PROD_WORKER_HOSTS` (index 1+)
3. Each host: load SSH key from vault → drain → deploy executor → deploy worker → verify healthy
4. Posts fleet-level Discord summary with per-host ✅/❌ status

### 3.4 `PROD_WORKER_HOSTS` Variable Format

Set in GitHub → Settings → Variables → `PROD_WORKER_HOSTS`:

```json
[
  {"name":"ant","host":"zombie-prod-worker-ant","vault_key":"zombie-prod-worker-ant"},
  {"name":"bird","host":"zombie-prod-worker-bird","vault_key":"zombie-prod-worker-bird"}
]
```

### 3.5 Prerequisites

- Create the `production-fleet` GitHub environment (Settings → Environments) with required reviewers
- Set `PROD_WORKER_HOSTS` GitHub variable with the JSON array above

---

## 4.0 Post-Deploy Verification

Run operator checks:

```bash
curl -sS https://api.usezombie.com/healthz
curl -sS https://api.usezombie.com/readyz | jq '.queue_dependency,.ready'
npx zombiectl login && npx zombiectl doctor
zombied doctor --format=json
```

---

## 5.0 Evidence Capture

Capture and store:

1. `release.yml` run URL
2. PROD health/ready output
3. worker rollout logs (`ant` + `bird`)
4. GitHub Release URL + attached binaries
5. post-deploy doctor output

Recommended evidence location:

- `docs/evidence/M3_002_PROD_DEPLOY_<YYYYMMDD>.md`

---

## 6.0 CLI PROD Smoke Gate

Run the CLI smoke against PROD after `/healthz` and `/readyz` are green:

```bash
export ZOMBIE_API_URL=https://api.<domain>

npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

Confirm:
- run reaches `completed` with a `pr_url`
- spec-to-PR latency under 5 minutes
- no errors in `runs list` output

---

## 7.0 Exit Criteria

- `release.yml` fully green: binaries, docker, npm, GitHub Release, deploy-prod-api, deploy-prod-canary, deploy-prod-fleet all pass
- PROD `/healthz` and `/readyz` green
- workers drained gracefully then redeployed over Tailscale SSH; `zombied-executor` + `zombied-worker` systemd services active; run queue consumed
- canary host verified healthy before fleet approval gate
- CLI PROD smoke complete (§6.0)
- evidence recorded (see M7_003_PROD_ACCEPTANCE.md §9.0)

If any gate fails, stop and fix before cutting a new tag.
