# M26_001: Acceptance Gate — CLI, Worker, and UI (DEV + PROD)

**Prototype:** v1.0.0
**Milestone:** M26
**Workstream:** 001
**Date:** Apr 04, 2026
**Status:** IN_PROGRESS
**Branch:** v0.3.1-prerelease-readiness
**Priority:** P0 — Release gate
**Depends on:** M7_001_DEV_ACCEPTANCE_ENVIRONMENT (DONE), M7_003_PROD_ACCEPTANCE_ENVIRONMENT (DONE), M7_005_NETWORK_CONNECTIVITY (DONE)

> Consolidates the remaining acceptance items from M7_001 (DEV CLI) and M7_003 (PROD CLI, workers, UI) into a single spec now that both environments are verified green.

---

## 1.0 DEV — CLI Acceptance

**Status:** PENDING

Full CLI-driven end-to-end acceptance run against DEV. Carried forward from M7_001 §6.0.

**Dimensions:**
- 1.1 PENDING Authenticate against DEV Clerk
- 1.2 PENDING Connect acceptance repo via `workspace add`
- 1.3 PENDING Sync specs from `docs/spec/`
- 1.4 PENDING Trigger a run and confirm it reaches terminal state
- 1.5 PENDING Verify run appears in list with correct status and PR linkage
- 1.6 PENDING Spec-to-PR latency recorded; must be under 5 minutes on DEV infra
- 1.7 PENDING `zombied doctor` output clean

```bash
export ZOMBIE_API_URL=https://api-dev.usezombie.com

npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run --spec <spec-file>
npx zombiectl runs list
npx zombiectl doctor
```

---

## 2.0 PROD — Version and Release Gate

**Status:** PENDING

A git tag matching the `VERSION` file triggers `release.yml`. Carried forward from M7_003 §1.0 and §2.0.

**Dimensions:**
- 2.1 PENDING Bump `VERSION` file to target release version (e.g. `0.4.0`)
- 2.2 PENDING Update `CHANGELOG.md` with release section `## [0.4.0]`
- 2.3 PENDING Push git tag `v0.4.0` — `release.yml` verifies tag matches `VERSION`
- 2.4 PENDING `verify-tag` CI job passes
- 2.5 PENDING `binaries` CI job passes for all 4 targets
- 2.6 PENDING Docker image pushed to GHCR: `ghcr.io/usezombie/zombied:latest`, `:0.4.0`, `:0.4.0-<sha>`
- 2.7 PENDING GitHub Release created with CHANGELOG excerpt and binary tarballs
- 2.8 PENDING `zombiectl` published to npm with provenance

```bash
git tag v0.4.0
git push origin v0.4.0
```

---

## 3.0 PROD — Worker Node Gate

**Status:** PENDING

Worker nodes `zombie-prod-worker-ant` and `zombie-prod-worker-bird` are bare-metal nodes reachable via Tailscale SSH. Carried forward from M7_003 §6.0.

**Dimensions:**
- 3.1 PENDING `/opt/zombie/deploy.sh` written on each node — pulls latest image, restarts worker process
- 3.2 PENDING `deploy-prod` CI SSH step succeeds for both nodes
- 3.3 PENDING Worker process starts, connects to DB and Redis, begins consuming run queue
- 3.4 PENDING Worker health confirmed: at least one test run dispatched and consumed by a worker node

---

## 4.0 PROD — UI Smoke Gate

**Status:** PENDING

Vercel PROD deployments for app and website. Carried forward from M7_003 §7.0.

**Dimensions:**
- 4.1 PENDING `smoke-post-deploy.yml` CI run green for `usezombie-app` PROD deployment
- 4.2 PENDING `smoke-post-deploy.yml` CI run green for `usezombie-website` PROD deployment
- 4.3 PENDING `https://app.usezombie.com` loads; Clerk auth flow reachable
- 4.4 PENDING `https://usezombie.com` loads; no broken assets

---

## 5.0 PROD — CLI Acceptance

**Status:** PENDING

CLI smoke run against PROD API. Carried forward from M7_003 §8.0.

**Dimensions:**
- 5.1 PENDING Authenticate against PROD Clerk
- 5.2 PENDING `workspace add` creates workspace on PROD
- 5.3 PENDING `specs sync` uploads and confirms spec count
- 5.4 PENDING `run` triggers and completes; PR opened on acceptance repo
- 5.5 PENDING `runs list` shows completed run with `pr_url`
- 5.6 PENDING Spec-to-PR latency under 5 minutes on PROD infra

```bash
export ZOMBIE_API_URL=https://api.usezombie.com

npx zombiectl login
npx zombiectl workspace add <ACCEPTANCE_REPO_URL>
npx zombiectl specs sync docs/spec/
npx zombiectl run
npx zombiectl runs list
```

---

## 6.0 Evidence Capture

**Status:** PENDING

**Dimensions:**
- 6.1 PENDING CI run ID for the full `release.yml` green run recorded
- 6.2 PENDING GitHub Release URL with binary tarballs and CHANGELOG excerpt
- 6.3 PENDING DEV CLI acceptance terminal output
- 6.4 PENDING PROD CLI acceptance terminal output
- 6.5 PENDING Worker rollout logs (ant + bird)
- 6.6 PENDING `healthz` + `readyz` snapshots for both environments

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [ ] 7.1 DEV CLI acceptance completes: login → workspace add → specs sync → run → runs list → doctor
- [ ] 7.2 `release.yml` runs green on `v0.4.0` tag: binaries, docker, npm, GitHub Release
- [ ] 7.3 Worker nodes deployed via Tailscale SSH; run queue consumed
- [ ] 7.4 UI PROD smoke passes: app and website Vercel deployments green
- [ ] 7.5 PROD CLI acceptance completes: login → workspace add → specs sync → run → runs list
- [ ] 7.6 Spec-to-PR latency under 5 minutes on both DEV and PROD
- [ ] 7.7 Evidence artifact complete

---

## 8.0 Out of Scope

- Environment/infrastructure verification (→ M7_001_DEV_ACCEPTANCE_ENVIRONMENT, M7_003_PROD_ACCEPTANCE_ENVIRONMENT, M7_005_NETWORK_CONNECTIVITY — all DONE)
- Post-launch SRE runbook expansion
- Multi-region PROD rollout beyond v1 contract
