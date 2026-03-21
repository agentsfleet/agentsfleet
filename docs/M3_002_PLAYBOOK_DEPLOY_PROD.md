# M3_002: Playbook — Deploy PROD

**Milestone:** M3
**Workstream:** 002
**Updated:** Mar 20, 2026
**Owner:** Agent
**Prerequisite:** `docs/M3_001_PLAYBOOK_DEPLOY_DEV.md` completed with evidence

This is the canonical step-by-step PROD deployment runbook.

---

## 1.0 Preflight Gate

1. Validate PROD credentials:

```bash
ENV=prod ./scripts/check-credentials.sh
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
7. `deploy-prod`

---

## 3.0 Deploy-Prod Behavior

`deploy-prod` must execute all of:

1. Load secrets from 1Password (`tailscale/authkey`, per-node worker keys)
2. Join tailnet in CI (`tailscale/github-action@v3`)
3. Verify PROD API rollout:

```bash
curl -sf https://api.usezombie.com/healthz
curl -sf https://api.usezombie.com/readyz | jq -e '.ready == true'
```

4. SSH to `zombie-prod-worker-ant` and `zombie-prod-worker-bird` over Tailscale and run:

```bash
cd /opt/zombie && ./deploy.sh
```

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

- `release.yml` fully green: binaries, docker, npm, GitHub Release, deploy-prod all pass
- PROD `/healthz` and `/readyz` green
- workers redeployed over Tailscale SSH; run queue consumed
- CLI PROD smoke complete (§6.0)
- evidence recorded (see M7_003_PROD_ACCEPTANCE.md §9.0)

If any gate fails, stop and fix before cutting a new tag.
