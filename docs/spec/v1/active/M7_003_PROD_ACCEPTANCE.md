# M7_003: PROD Acceptance Gate

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 003
**Date:** Mar 20, 2026
**Status:** IN_PROGRESS
**Branch:** v0.3.1-prerelease-readiness
**Priority:** P0 — PROD release gate
**Batch:** B3 — after M7_002 (HTTPZ Migration) and M7_005 (Network Connectivity)
**Depends on:** M7_001_DEV_ACCEPTANCE (DEV gate must be green before PROD rollout), M7_005_NETWORK_CONNECTIVITY (PROD tunnel + database + cache access), M7_002_HTTPZ_MIGRATION

> **Pre-condition (Mar 20, 2026):** M7_001 §1.1 (Fly.io DEV setup + Cloudflare Tunnel) is the immediate unblocking action. PROD work cannot start until DEV acceptance gate is green.

---

## 1.0 Version Gate

**Status:** PENDING

A git tag matching the `VERSION` file triggers `release.yml`. The tag is the single source of truth for the release version.

**Dimensions:**
- 1.1 PENDING Bump `VERSION` file to target release version (e.g. `0.4.0`)
- 1.2 PENDING Update `CHANGELOG.md` with release section `## [0.4.0]`
- 1.3 PENDING Push git tag `v0.4.0` — `release.yml` verifies tag matches `VERSION` exactly and fails fast if not
- 1.4 PENDING `verify-tag` CI job passes: `tag v0.4.0 matches VERSION 0.4.0`

```bash
# After VERSION and CHANGELOG updated and committed:
git tag v0.4.0
git push origin v0.4.0
```

---

## 2.0 Binary and Image Gate

**Status:** PENDING

`release.yml` cross-compiles 4 targets and pushes the production GHCR image.

**Dimensions:**
- 2.1 PENDING `binaries` CI job passes for all 4 targets: `zombied-linux-amd64`, `zombied-linux-arm64`, `zombied-darwin-amd64`, `zombied-darwin-arm64`
- 2.2 PENDING `docker` CI job passes: `ghcr.io/usezombie/zombied:latest`, `zombied:0.4.0`, `zombied:0.4.0-<sha>` pushed to GHCR
- 2.3 PENDING GitHub Release created with CHANGELOG excerpt and all 4 binary tarballs attached
- 2.4 PENDING `zombiectl` published to npm with provenance: `npm install -g zombiectl@0.4.0` installs correctly

---

## 3.0 DEV Readiness Gate

**Status:** DONE

`verify-dev-gate` in `release.yml` blocks PROD rollout until DEV is healthy. M7_001_DEV_ACCEPTANCE must be complete before this gate can pass.

**Dimensions:**
- 3.1 DONE `https://api-dev.usezombie.com/healthz` returns 200
- 3.2 DONE `https://api-dev.usezombie.com/readyz` returns `{ "ready": true }`
- 3.3 PENDING `verify-dev-gate` CI job green — PROD rollout unblocked

**Evidence (Apr 02, 2026):**
```
$ curl -sf https://api-dev.usezombie.com/healthz
{"status":"ok","service":"zombied","database":"up","commit":"f5f3194"}

$ curl -sf https://api-dev.usezombie.com/readyz | jq '.ready'
true
```

---

## 4.0 PROD Environment Map

**Status:** DONE — documented; runtime connection pending (§5.0, §6.0)

| Service | URL | Image / Deployable |
|---------|-----|--------------------|
| API | `https://api.usezombie.com` | `ghcr.io/usezombie/zombied:latest` (Fly.io `zombied-prod`) |
| API (version-pinned) | same | `ghcr.io/usezombie/zombied:0.4.0` |
| Worker ant | Tailscale only | same `latest` pulled on node |
| Worker bird | Tailscale only | same `latest` pulled on node |
| App / Dashboard | `https://app.usezombie.com` | `usezombie-app` Vercel project |
| Website | `https://usezombie.com` | `usezombie-website` Vercel project |
| Agents site | `https://agents.usezombie.com` | `usezombie-agents` Vercel project |

---

## 5.0 Fly.io PROD API Gate

**Status:** IN_PROGRESS — API healthy, vault verified; CI deploy job not yet run

**Dimensions:**
- 5.1 DONE Fly.io PROD app (`zombied-prod`) deployed from `ghcr.io/usezombie/zombied:latest`; Cloudflare Tunnel `zombied-prod` routes `api.usezombie.com` → Fly private network. See M2_002 §2.0.
- 5.2 PENDING `deploy-prod` CI job polls `https://api.usezombie.com/healthz` (24 attempts × 10s); must return 200. Print HTTP status + response body per attempt so Fly-not-deployed vs zombied-crashed are distinguishable.
- 5.3 DONE `https://api.usezombie.com/readyz` returns `{ "ready": true }`
- 5.4 DONE PROD vault items green: `planetscale-prod`, `tailscale`, `zombie-prod-worker-{ant,bird}/ssh-private-key`, `clerk-prod`, `fly-api-token`, `cloudflare-tunnel-prod`

**Evidence (Apr 04, 2026):**
```
$ curl -sf https://api.usezombie.com/healthz
{"status":"ok","service":"zombied","database":"up","commit":"7eee364"}

$ curl -sf https://api.usezombie.com/readyz | jq '{ready, database, queue_dependency}'
{"ready":true,"database":true,"queue_dependency":true}
```

```bash
curl -sf https://api.usezombie.com/healthz
curl -sf https://api.usezombie.com/readyz | jq -e '.ready == true'
```

---

## 6.0 Worker Node Gate

**Status:** PENDING

Worker nodes `zombie-prod-worker-ant` and `zombie-prod-worker-bird` are bare-metal nodes reachable via Tailscale SSH. `deploy.sh` must be written, bootstrapped, and tested before this gate can pass.

**Dimensions:**
- 6.1 PENDING `/opt/zombie/deploy.sh` written on each node — pulls `ghcr.io/usezombie/zombied:latest`, restarts worker process (systemd or docker restart)
- 6.2 PENDING `deploy-prod` CI SSH step succeeds for both nodes: `ssh zombie-prod-worker-ant "cd /opt/zombie && ./deploy.sh"`
- 6.3 PENDING Worker process starts, connects to DB and Redis, begins consuming run queue
- 6.4 PENDING Worker health confirmed: at least one test run dispatched and consumed by a worker node

---

## 7.0 UI PROD Smoke Gate

**Status:** PENDING — Vercel PROD deployments not yet live

`smoke-post-deploy.yml` fires automatically on Vercel `deployment_status` events for each PROD project.

**Dimensions:**
- 7.1 PENDING `smoke-post-deploy.yml` CI run green for `usezombie-app` PROD deployment
- 7.2 PENDING `smoke-post-deploy.yml` CI run green for `usezombie-website` PROD deployment
- 7.3 PENDING `https://app.usezombie.com` loads; Clerk auth flow reachable
- 7.4 PENDING `https://usezombie.com` loads; no broken assets

---

## 8.0 CLI PROD Smoke Gate

**Status:** PENDING

CLI smoke run against PROD API. Mirrors M7_001 §6.0 but targets `api.usezombie.com`.

**Dimensions:**
- 8.1 PENDING Authenticate against PROD Clerk
- 8.2 PENDING `workspace add` creates workspace on PROD
- 8.3 PENDING `specs sync` uploads and confirms spec count
- 8.4 PENDING `run` triggers and completes; PR opened on acceptance repo
- 8.5 PENDING `runs list` shows completed run with `pr_url`
- 8.6 PENDING Spec-to-PR latency under 5 minutes on PROD infra

```bash
export ZOMBIE_API_URL=https://api.usezombie.com

npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

---

## 9.0 Evidence Capture

**Status:** PENDING

**Dimensions:**
- 9.1 PENDING CI run ID for the full `release.yml` green run recorded
- 9.2 PENDING GitHub Release URL with binary tarballs and CHANGELOG excerpt
- 9.3 PENDING CLI PROD smoke terminal output in `docs/evidence/M7_003_PROD_ACCEPTANCE_EVIDENCE.md`
- 9.4 PENDING `healthz` + `readyz` snapshots for both API and workers

---

## 10.0 Acceptance Criteria

**Status:** PENDING

- [ ] 10.1 `release.yml` runs green on `v0.4.0` tag: binaries, docker, npm, GitHub Release all pass
- [x] 10.2 Fly.io PROD API healthy (`zombied-prod`); Cloudflare Tunnel wired; `healthz` + `readyz` green
- [ ] 10.3 Worker nodes deployed via Tailscale SSH; run queue consumed
- [ ] 10.4 UI PROD smoke passes: app and website Vercel deployments green
- [ ] 10.5 CLI PROD smoke completes: login → workspace add → specs sync → run → runs list
- [ ] 10.6 Spec-to-PR latency under 5 minutes on PROD
- [ ] 10.7 Evidence artifact complete in `docs/evidence/M7_003_PROD_ACCEPTANCE_EVIDENCE.md`

---

## 11.0 Out of Scope

- DEV acceptance execution (→ M7_001_DEV_ACCEPTANCE.md)
- Post-launch SRE runbook expansion
- Multi-region PROD rollout beyond v1 contract
