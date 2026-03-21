# M7_001: DEV Acceptance Gate

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 001
**Date:** Mar 20, 2026
**Status:** PENDING
**Priority:** P0 — DEV release gate; blocks M7_003 (PROD Acceptance)
**Depends on:** M7_001_DEPLOY (deploy pipeline wired), M6_006 (CLI hardened, DB gate passing)
**Successor:** M7_003_PROD_ACCEPTANCE.md

> **Status (Mar 20, 2026):** GHCR package set to public ✅. Cloudflare CNAME created ✅. Migrating from Railway to Fly.io + Cloudflare Tunnel for true origin shielding (see M2_002 §2.0). `deploy-dev.yml` needs updating to use `fly deploy` instead of Railway GraphQL trigger.

---

## 1.0 Infrastructure Gate

**Status:** PENDING

Fly.io DEV app must be running, reachable at `api-dev.usezombie.com` via Cloudflare Tunnel, and the full deploy-dev pipeline must run green end-to-end.

**Dimensions:**
- 1.1a **DONE** GHCR package `ghcr.io/usezombie/zombied` set to **public** ✅
- 1.1b **DONE** Cloudflare CNAME `api-dev.usezombie.com` created ✅ — will be updated to tunnel CNAME once Fly.io is wired
- 1.1c **PENDING [agent]** Create Fly.io apps (`zombied-dev`, `zombied-dev-worker`, `cloudflared-dev`), set secrets from 1Password, deploy from GHCR. See M2_002 §2.1–2.3. No public Fly port — all traffic via Cloudflare Tunnel only.
- 1.1d **PENDING [agent]** Create Cloudflare Tunnel `zombied-dev`, route `api-dev.usezombie.com` → tunnel. See M2_002 §2.4. Replaces CNAME + Transform Rule hack.
- 1.1e **PENDING [agent]** Update `deploy-dev.yml`: replace `trigger-railway-dev` job with `fly deploy --app zombied-dev --image ghcr.io/usezombie/zombied:dev-latest`. Store `fly-api-token` in vault + `FLY_API_TOKEN` GitHub secret.
- 1.2 PENDING Verify `deploy-dev.yml` `build-dev` step completes: Zig cross-compile → `make push-dev` → GHCR push succeeds
- 1.3 PENDING Verify `deploy-fly-dev` step passes: `fly deploy` returns exit 0, machine healthy
- 1.4 PENDING Verify `verify-dev` step passes: `https://api-dev.usezombie.com/healthz` returns 200 within 180s. Prints HTTP status + body per attempt for diagnostics.
- 1.5 PENDING Verify `verify-dev` step passes: `https://api-dev.usezombie.com/readyz` returns `{ "ready": true }`
- 1.6 PENDING DEV vault items all green in `check-credentials.sh`: `clerk-dev`, `vercel-api-token`, `planetscale-dev`, `upstash-dev`, `fly-api-token`, `cloudflare-tunnel-dev`

---

## 2.0 DEV Environment Map

**Status:** DONE — documented; runtime connection pending (§1.0)

| Service | URL | Image |
|---------|-----|-------|
| API | `https://api-dev.usezombie.com` | `ghcr.io/usezombie/zombied:dev-latest` (Fly.io `zombied-dev`) |
| API (sha-pinned) | same | `ghcr.io/usezombie/zombied:0.x.0-dev-<sha>` |
| App / Dashboard | Vercel preview URL (per-commit) | `usezombie-app` Vercel project |
| Website | Vercel preview URL (per-commit) | `usezombie-website` Vercel project |

---

## 3.0 API Health Gate

**Status:** PENDING

**Dimensions:**
- 3.1 PENDING `GET https://api-dev.usezombie.com/healthz` → HTTP 200
- 3.2 PENDING `GET https://api-dev.usezombie.com/readyz` → `{ "ready": true }`
- 3.3 PENDING `zombied doctor` output: all checks `[OK]` — DB connectivity, Redis connectivity, Clerk auth, vault key present

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
npx zombiectl doctor --api-url https://api-dev.usezombie.com
```

---

## 4.0 UI Smoke Gate

**Status:** PENDING

Vercel preview smoke tests fire automatically via `smoke-post-deploy.yml` on each Vercel deploy. Manual verification steps below.

**Dimensions:**
- 4.1 PENDING App Vercel preview URL loads without error; Clerk auth flow reachable
- 4.2 PENDING Website Vercel preview URL loads; no broken assets or links
- 4.3 PENDING `smoke-post-deploy.yml` CI run green for `usezombie-app` deployment event
- 4.4 PENDING `smoke-post-deploy.yml` CI run green for `usezombie-website` deployment event

---

## 5.0 Playwright QA Smoke Gate

**Status:** PENDING

`qa-dev` step in `deploy-dev.yml` runs Playwright smoke suite against `https://api-dev.usezombie.com` after Fly.io DEV goes green.

**Dimensions:**
- 5.1 PENDING `qa-dev` CI step passes end-to-end against live DEV API
- 5.2 PENDING Playwright report artifact uploaded to CI (`qa-dev-<sha>`)
- 5.3 PENDING No regressions in smoke suite compared to prior green run

---

## 6.0 CLI Acceptance Gate

**Status:** PENDING

Full CLI-driven end-to-end acceptance run against DEV. Commands run in order; each must succeed before the next.

**Dimensions:**
- 6.1 PENDING Authenticate against DEV Clerk
- 6.2 PENDING Connect acceptance repo
- 6.3 PENDING Sync specs from `docs/spec/`
- 6.4 PENDING Trigger a run and confirm it reaches terminal state
- 6.5 PENDING Verify run appears in list with correct status and PR linkage
- 6.6 PENDING Spec-to-PR latency recorded; must be under 5 minutes on DEV infra

```bash
export ZOMBIE_API_URL=https://api-dev.usezombie.com

npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

**Expected outcomes:**
- `login` → Clerk auth token stored in local config
- `workspace add` → workspace created, GitHub app installed on acceptance repo
- `specs sync` → spec files uploaded, sync confirmation with spec count
- `run` → run ID returned, status transitions to `running` then `completed`
- `runs list` → run appears with `status: completed`, `pr_url` present

---

## 7.0 Evidence Capture

**Status:** PENDING

**Dimensions:**
- 7.1 PENDING CI artifact: full `deploy-dev.yml` run log for the green DEV deploy (`databaseId` recorded)
- 7.2 PENDING CI artifact: Playwright report from `qa-dev` step
- 7.3 PENDING Terminal output from all CLI commands in §6.0 captured and stored in `docs/evidence/M7_001_DEV_ACCEPTANCE_EVIDENCE.md`
- 7.4 PENDING `zombied doctor` output snapshot included in evidence

---

## 8.0 Acceptance Criteria

**Status:** PENDING

- [ ] 8.1 Fly.io DEV running (`zombied-dev`); Cloudflare Tunnel wired; `deploy-dev.yml` pipeline runs green on main push
- [ ] 8.2 `healthz` + `readyz` return green on `api-dev.usezombie.com`
- [ ] 8.3 Playwright QA smoke passes against live DEV API
- [ ] 8.4 UI smoke passes for app and website Vercel previews
- [ ] 8.5 CLI acceptance run completes: login → workspace add → specs sync → run → runs list
- [ ] 8.6 Spec-to-PR latency under 5 minutes
- [ ] 8.7 Evidence artifact complete in `docs/evidence/M7_001_DEV_ACCEPTANCE_EVIDENCE.md`

---

## 9.0 Out of Scope

- PROD deployment (→ M7_003_PROD_ACCEPTANCE.md)
- UI-driven acceptance flows
- Performance soak testing beyond the 5-minute latency gate
