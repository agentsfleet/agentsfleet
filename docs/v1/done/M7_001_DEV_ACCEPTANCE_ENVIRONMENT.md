# M7_001: DEV Acceptance Gate

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 001
**Date:** Mar 20, 2026
**Status:** DONE — environment verified; CLI acceptance moved to M26_001_ACCEPTANCE.md
**Priority:** P0 — DEV release gate; blocks M7_003 (PROD Acceptance)
**Depends on:** M12_003 (NullClaw invocation — executor runtime complete)
**Successor:** M26_001_ACCEPTANCE.md

> **Status (Mar 21, 2026):** GHCR package set to public ✅. Cloudflare Tunnel wired ✅. Fly.io DEV deployed ✅. `deploy-dev.yml` updated to use `fly deploy` + Cloudflare Tunnel verification. Railway fully removed.

> **Status (Mar 27, 2026):** Infrastructure, API health, UI smoke, and Playwright QA gates all pass. `deploy-dev.yml` pipeline fully green (run 23630635008). §6.0 CLI Acceptance blocked — `zombiectl` CLI not yet built/published. §3.3 `zombied doctor` deferred (requires CLI or SSH to Fly machine).

> **Status (Mar 27, 2026 — credential rotation + deploy script):** Vercel bypass secrets rotated in ZMB_CD_PROD vault. Upstash DEV Redis password rotated; agent derived `api-url`/`worker-url` from base URL in ZMB_CD_DEV vault. `deploy/baremetal/deploy.sh` built with dual-mode binary acquisition and Discord notifications. CI `deploy-dev-worker` job updated to scp compiled binaries and use deploy.sh. `zombied-executor` binary added to build-dev artifact upload. Worker fleet drain design deferred to M13_001.

---

## 1.0 Infrastructure Gate

**Status:** DONE

Fly.io DEV app must be running, reachable at `api-dev.usezombie.com` via Cloudflare Tunnel, and the full deploy-dev pipeline must run green end-to-end.

**Dimensions:**
- 1.1a ✅ DONE GHCR package `ghcr.io/usezombie/zombied` set to **public**
- 1.1b ✅ DONE Cloudflare CNAME `api-dev.usezombie.com` → tunnel CNAME
- 1.1c ✅ DONE Fly.io apps created (`zombied-dev`, `cloudflared-dev`), secrets set from 1Password, deployed from GHCR. No public Fly port — all traffic via Cloudflare Tunnel only.
- 1.1d ✅ DONE Cloudflare Tunnel `zombied-dev` wired, `api-dev.usezombie.com` → Fly 6PN `zombied-dev.internal:3000`
- 1.1e ✅ DONE `deploy-dev.yml` updated: `deploy-fly-dev` job runs `fly deploy --app zombied-dev --image ghcr.io/usezombie/zombied:dev-latest`. `fly-api-token` stored in vault, loaded via `OP_SERVICE_ACCOUNT_TOKEN` in CI.
- 1.2 ✅ DONE `deploy-dev.yml` `build-dev` step completes: Zig cross-compile → `make push-dev` → GHCR push succeeds
- 1.3 ✅ DONE `deploy-fly-dev` step passes: `fly deploy` returns exit 0, machines deployed (v5)
- 1.4 ✅ DONE `verify-dev` step passes: `/healthz` returns 200 within 10s (run 23630635008)
- 1.5 ✅ DONE `verify-dev` step passes: `/readyz` returns `{ "ready": true }` (run 23630635008)
- 1.6 ✅ DONE DEV vault items all green in `check-credentials.sh`: `clerk-dev`, `vercel-api-token`, `planetscale-dev`, `upstash-dev`, `fly-api-token`, `cloudflare-tunnel-dev`, `encryption-master-key`
- 1.7 ✅ DONE `deploy/baremetal/deploy.sh` built: function-based, dual-mode (local binary or GitHub Release download), Discord notification, idempotent version check. CI `deploy-dev-worker` job updated to scp binaries + call deploy.sh with local path.

---

## 2.0 DEV Environment Map

**Status:** DONE

| Service | URL | Image |
|---------|-----|-------|
| API | `https://api-dev.usezombie.com` | `ghcr.io/usezombie/zombied:dev-latest` (Fly.io `zombied-dev`) |
| API (sha-pinned) | same | `ghcr.io/usezombie/zombied:0.x.0-dev-<sha>` |
| App / Dashboard | Vercel preview URL (per-commit) | `usezombie-app` Vercel project |
| Website | Vercel preview URL (per-commit) | `usezombie-website` Vercel project |

---

## 3.0 API Health Gate

**Status:** IN_PROGRESS (3.3 blocked on CLI)

**Dimensions:**
- 3.1 ✅ DONE `GET https://api-dev.usezombie.com/healthz` → `{"status":"ok","service":"zombied","database":"up"}`
- 3.2 ✅ DONE `GET https://api-dev.usezombie.com/readyz` → `{"ready":true,"database":true,"worker":true,"queue_dependency":true}`
- 3.3 PENDING `zombied doctor` output — run via `./zombiectl/bin/zombiectl.js doctor --api-url https://api-dev.usezombie.com`

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
./zombiectl/bin/zombiectl.js doctor --api-url https://api-dev.usezombie.com
```

---

## 4.0 UI Smoke Gate

**Status:** DONE

Vercel preview smoke tests fire automatically via `smoke-post-deploy.yml` on each Vercel deploy.

**Dimensions:**
- 4.1 ✅ DONE App Vercel preview URL loads; Clerk auth flow reachable (307 → `/sign-in`)
- 4.2 ✅ DONE Website Vercel preview URL loads; deployment marked "Ready" by Vercel
- 4.3 ✅ DONE `smoke-post-deploy.yml` CI run green for `usezombie-app` deployment event
- 4.4 ✅ DONE `smoke-post-deploy.yml` CI run green for `usezombie-website` deployment event

---

## 5.0 Playwright QA Smoke Gate

**Status:** DONE

`qa-dev` step in `deploy-dev.yml` runs Playwright smoke suite against `https://usezombie-app.vercel.app` (Vercel app) after Fly.io DEV goes green.

**Dimensions:**
- 5.1 ✅ DONE `qa-dev` CI step passes end-to-end (run 23630635008, 4 tests, 2 projects)
- 5.2 ✅ DONE Playwright report artifact uploaded: `qa-dev-ccbad03...` (artifact 6136852031)
- 5.3 ✅ DONE No regressions — first green run establishes baseline

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

# Run from repo root — no npm publish needed
./zombiectl/bin/zombiectl.js login
./zombiectl/bin/zombiectl.js workspace add <ACCEPTANCE_REPO_URL>
./zombiectl/bin/zombiectl.js specs sync docs/spec/
./zombiectl/bin/zombiectl.js run --spec docs/spec/v1/M15_001_SELF_SERVE_ROLE_ASSIGNMENT.md
./zombiectl/bin/zombiectl.js runs list
```

**Local test suite (run before acceptance):**
```bash
cd zombiectl && bun run test
```

**Expected outcomes:**
- `login` → Clerk auth token stored in local config
- `workspace add` → workspace created, GitHub app installed on acceptance repo
- `specs sync` → spec files uploaded, sync confirmation with spec count
- `run --spec` → run ID returned, status transitions to `running` then `completed`
- `runs list` → run appears with `status: completed`, `pr_url` present

---

## 7.0 Evidence Capture

**Status:** IN_PROGRESS (7.3, 7.4 blocked on CLI)

**Dimensions:**
- 7.1 ✅ DONE CI artifact: `deploy-dev.yml` run 23630635008 — all jobs green
- 7.2 ✅ DONE CI artifact: Playwright report `qa-dev-ccbad03...` (artifact ID 6136852031)
- 7.3 PENDING Terminal output from CLI commands in §6.0 — run via `./zombiectl/bin/zombiectl.js`
- 7.4 PENDING `zombied doctor` output — run via `./zombiectl/bin/zombiectl.js doctor`

---

## 8.0 Acceptance Criteria

**Status:** IN_PROGRESS (8.5, 8.6, 8.7 blocked on CLI)

- [x] 8.1 Fly.io DEV running (`zombied-dev`); Cloudflare Tunnel wired; `deploy-dev.yml` pipeline runs green on main push
- [x] 8.2 `healthz` + `readyz` return green on `api-dev.usezombie.com`
- [x] 8.3 Playwright QA smoke passes against live DEV app (`usezombie-app.vercel.app`)
- [x] 8.4 UI smoke passes for app and website Vercel previews
- [ ] 8.5 CLI acceptance run completes: login → workspace add → specs sync → run → runs list — via `./zombiectl/bin/zombiectl.js`
- [ ] 8.6 Spec-to-PR latency under 5 minutes
- [ ] 8.7 Evidence artifact complete in `docs/evidence/M7_001_DEV_ACCEPTANCE_EVIDENCE.md`

---

## 9.0 Out of Scope

- PROD deployment (→ M7_003_PROD_ACCEPTANCE.md)
- UI-driven acceptance flows
- Performance soak testing beyond the 5-minute latency gate
