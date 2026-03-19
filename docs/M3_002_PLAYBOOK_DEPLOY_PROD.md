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

2. Confirm DEV is green (M3_001 complete).
3. Confirm release inputs are ready (`VERSION`, `CHANGELOG`).

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

4. SSH to `zombie-worker-ant` and `zombie-worker-bird` over Tailscale and run:

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

## 6.0 Exit Criteria

- release workflow complete and green
- PROD `/healthz` and `/readyz` pass
- workers successfully redeployed over Tailscale SSH
- evidence recorded

If any gate fails, stop and fix before next tag.
