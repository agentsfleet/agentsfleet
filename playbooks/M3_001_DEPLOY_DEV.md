# M3_001: Playbook — Deploy DEV

**Milestone:** M3
**Workstream:** 001
**Updated:** Mar 27, 2026
**Owner:** Agent
**Prerequisite:** `playbooks/M1_001_BOOTSTRAP.md`, `playbooks/M2_001_PREFLIGHT.md`, `playbooks/M2_002_PRIMING_INFRA.md`

This is the canonical step-by-step DEV deployment runbook.

---

## 1.0 Preflight Gate

**Status:** ✅ DONE

1. Ensure required credentials exist:

```bash
ENV=dev ./playbooks/gates/check-credentials.sh
```

2. Ensure branch is clean and validated:

```bash
make lint
make test
```

3. Ensure `deploy-dev.yml` is present and healthy in `main`.

4. Ensure `cloudflared-dev` Fly app is deployed (one-time prerequisite):

```bash
# Check if machines exist
flyctl machine list --app cloudflared-dev

# If no machines — deploy once; CI handles restarts after this
flyctl deploy --app cloudflared-dev --config deploy/fly/cloudflared-dev/fly.toml

# Verify TUNNEL_TOKEN secret is set
flyctl secrets list --app cloudflared-dev | grep TUNNEL_TOKEN
```

> After the first deploy, CI's `verify-dev` job automatically restarts or redeploys `cloudflared-dev` if machines are down. No manual intervention needed on subsequent runs.

---

## 2.0 Trigger DEV Deploy

**Status:** ✅ DONE

1. Merge/push changes to `main`.
2. Confirm GitHub Actions workflow `.github/workflows/deploy-dev.yml` starts.

Expected DEV pipeline order:

1. `check-credentials`
2. `build-dev` — cross-compiles and pushes `dev-latest` to GHCR
3. `deploy-fly-dev` — `flyctl deploy --app zombied-dev --image ghcr.io/usezombie/zombied:dev-latest`
4. `verify-dev` — polls `https://api-dev.usezombie.com/healthz` until 200
5. `qa-dev` — Playwright smoke suite against `https://usezombie-app.vercel.app`
6. `notify` — Discord

### 2.1 Immediate Fix — Fly Crash Loop on Role-Separated DB URL Guard

Symptom in Fly logs:

```text
fatal: DATABASE_URL_API and DATABASE_URL_WORKER must differ (role separation required)
or migration release command fails with missing/invalid `DATABASE_URL_MIGRATOR`
```

Fix:

1. Set different Fly secrets for API vs worker DB URLs on `zombied-dev`:

```bash
fly secrets set \
  DATABASE_URL_API='...' \
  DATABASE_URL_WORKER='...' \
  DATABASE_URL_MIGRATOR='...' \
  --app zombied-dev
```

2. Redeploy:

```bash
gh workflow run deploy-dev.yml
```

3. Verify:

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
```

---

## 3.0 Runtime Verification

**Status:** ✅ DONE

Run after workflow is green:

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
```

Optional operator checks (requires `zombiectl` CLI — not yet available):

```bash
npx zombiectl doctor
zombied doctor --format=json
```

---

## 4.0 Smoke Gate

**Status:** ✅ DONE

DEV smoke must pass from CI (`qa-dev` job).

If smoke fails:

1. Open failing action run logs.
2. Fix issue on branch.
3. Merge to `main` and re-run deploy-dev pipeline.

No release tagging until DEV is green.

---

## 5.0 Evidence Capture

**Status:** ✅ DONE (CI evidence; CLI evidence blocked on `zombiectl`)

Captured:

1. `deploy-dev.yml` run 23630635008 — all green
2. `verify-dev` output: `/healthz` 200, `/readyz` `ready:true`
3. QA smoke artifact: `qa-dev-ccbad03...` (artifact ID 6136852031)
4. Discord notify: success embed sent

Evidence location:

- `docs/evidence/M3_001_DEV_DEPLOY_<YYYYMMDD>.md`

---

## 6.0 CLI Acceptance Gate

**Status:** PENDING — blocked: `zombiectl` CLI not yet built/published

Run the full CLI acceptance flow against DEV after the pipeline is green:

```bash
export ZOMBIE_API_URL=https://api-dev.usezombie.com

npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

Expected outcomes:
- `login` — Clerk auth token stored in local config
- `workspace add` — workspace created, GitHub App installed on acceptance repo
- `specs sync` — spec files uploaded, count confirmed
- `run` — run ID returned; status transitions to `running` then `completed`; PR opened on acceptance repo
- `runs list` — run appears with `status: completed` and `pr_url` present

Spec-to-PR latency must be under 5 minutes. Record the actual time in evidence.

---

## 7.0 Exit Criteria

- ✅ DEV pipeline fully green
- ✅ `/healthz` and `/readyz` return success
- ✅ smoke tests pass
- ⏳ CLI acceptance run complete (§6.0) — **blocked on `zombiectl`**
- ✅ evidence recorded (see M7_001_DEV_ACCEPTANCE.md §7.0)

When all pass, continue to `playbooks/M3_002_DEPLOY_PROD.md`.
