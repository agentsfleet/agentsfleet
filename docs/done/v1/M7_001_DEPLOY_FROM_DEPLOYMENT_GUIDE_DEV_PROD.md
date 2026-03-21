# M7_001: Deploy From Deployment Guide (DEV And PROD)

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 001
**Date:** Mar 08, 2026
**Updated:** Mar 20, 2026: 10:00 AM
**Status:** DONE
**Priority:** P0 — deployment execution gate
**Depends on:** M6_006 (Validate v1 Acceptance E2E Gate), M4_007 (Runtime Environment Contract), M7_002 (Documentation Production And Publish)

---

## 1.0 DEV Deployment Pipeline

**Status:** DONE

CI/CD pipeline fully wired. Railway DEV connection is the remaining runtime dependency, tracked in M7_001_DEV_ACCEPTANCE.md.

**Dimensions:**
- 1.1 ✅ DONE Gitleaks wired as hard prerequisite in release DAG (`docker` job `needs: [verify-tag, binaries]`; runs on all PR/push lanes). `.gitleaks.toml` updated to ignore `.tmp/`, `zig-cache/`, `.zig-cache/`, `zig-out/` paths.
- 1.2 ✅ DONE `Dockerfile` refactored to binary-copy model — cross-compiles on CI runner, `COPY dist/zombied-linux-${TARGETARCH}` into `debian:trixie-slim`. No Zig toolchain inside Docker.
- 1.3 ✅ DONE `deploy-dev.yml` fully wired: main push → Zig cross-compile (amd64 + arm64) → `make push-dev` → `ghcr.io/usezombie/zombied:dev-latest` + `zombied:0.x.0-dev-<sha>` → Railway DEV healthz poll → Playwright QA smoke → Discord notify.
- 1.4 ✅ DONE `verify-dev` job polls `https://api-dev.usezombie.com/healthz` (18 attempts × 10s). Blocked on Railway DEV being connected to GHCR image — tracked in M7_001_DEV_ACCEPTANCE.md.
- 1.5 ✅ DONE Node.js 20 deprecation resolved: `jetsung/setup-zig@v1` used across all workflows; `mlugg/setup-zig@v2` retained only for `memleak.yml` (runs inside `debian:trixie-slim` container where shell-script actions fail); `1password/load-secrets-action` bumped to `@v3` (PR #61).

---

## 2.0 PROD Deployment Pipeline

**Status:** DONE

Tag-based release pipeline wired end-to-end. Worker node `deploy.sh` and PROD Railway connection are runtime dependencies tracked in M7_003_PROD_ACCEPTANCE.md.

**Dimensions:**
- 2.1 ✅ DONE `release.yml` triggered on `v*` tags. Gate: tag must match `VERSION` file exactly.
- 2.2 ✅ DONE Binaries cross-compiled for 4 targets (linux/amd64, linux/arm64, darwin/amd64, darwin/arm64), uploaded as artifacts, attached to GitHub Release with CHANGELOG excerpt.
- 2.3 ✅ DONE `make push` pushes `ghcr.io/usezombie/zombied:latest` + `zombied:vX.Y.Z` + `zombied:vX.Y.Z-<sha>` using prebuilt binaries from artifact download step.
- 2.4 ✅ DONE `verify-dev-gate` job blocks PROD rollout until `api-dev.usezombie.com/healthz` + `/readyz` are green — prevents deploying to PROD if DEV is unhealthy.
- 2.5 ✅ DONE `deploy-prod` job: loads Tailscale authkey + per-node SSH keys from `op://ZMB_CD_PROD`; joins Tailscale network; polls `api.usezombie.com/healthz` (24 attempts × 10s); SSHs into `zombie-worker-ant` + `zombie-worker-bird` and runs `/opt/zombie/deploy.sh`.
- 2.6 ✅ DONE `zombiectl` published to npm on every release tag via `npm publish --provenance`.

---

## 3.0 Deployment URL and Image Map

**Status:** DOCUMENTED — runtime connection tracked in M7_001_DEV_ACCEPTANCE.md §2.0 (DEV) and M7_003_PROD_ACCEPTANCE.md §4.0 (PROD)

| Environment | Service | URL | Image / Deployable |
|-------------|---------|-----|--------------------|
| DEV | API | `https://api-dev.usezombie.com` | `ghcr.io/usezombie/zombied:dev-latest` (Railway) |
| DEV | API (sha-pinned) | same | `ghcr.io/usezombie/zombied:0.x.0-dev-<sha>` |
| DEV | App/Dashboard | Vercel preview URL (per-commit) | `usezombie-app` Vercel project |
| DEV | Website | Vercel preview URL (per-commit) | `usezombie-website` Vercel project |
| PROD | API | `https://api.usezombie.com` | `ghcr.io/usezombie/zombied:latest` (Railway) |
| PROD | API (version-pinned) | same | `ghcr.io/usezombie/zombied:vX.Y.Z` |
| PROD | Worker ant | Tailscale only | same `latest` pulled on node |
| PROD | Worker bird | Tailscale only | same `latest` pulled on node |
| PROD | App/Dashboard | `https://app.usezombie.com` | `usezombie-app` Vercel project |
| PROD | Website | `https://usezombie.com` | `usezombie-website` Vercel project |
| PROD | Agents site | `https://agents.usezombie.com` | `usezombie-agents` Vercel project |

---

## 4.0 Deployment Guide Validation

**Status:** DONE

**Dimensions:**
- 4.1 ✅ DONE Ambiguity/drift log captured in milestone playbooks with concrete mismatches and correction policy
- 4.2 ✅ DONE Rebuilt deployment docs into chronological no-drift execution flow (`M1_001`, `M2_001`, `M2_002`, `M3_001`, `M3_002`)
- 4.3 ✅ DONE Vault references abstracted to `$VAULT_DEV`/`$VAULT_PROD`; human/agent split documented; redis-acl-* items removed (Upstash manages via dashboard)
- 4.4 ✅ DONE `scripts/check-credentials.sh` — portable credential gate for all vault items; DEV vault fully green

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 DEV deploy pipeline wired and pushing to GHCR on every main push
- [x] 5.2 PROD deploy pipeline wired: tag → binaries → docker → npm → GitHub Release → Railway PROD → worker SSH
- [x] 5.3 Deployment URL and image map documented and accurate
- [x] 5.4 Node.js 20 deprecation warnings resolved across all 12 CI workflows
- [x] 5.5 Deployment docs corrected for all execution drift found

**Pending runtime execution** (tracked separately):
- Railway DEV connected to GHCR → M7_001_DEV_ACCEPTANCE.md
- Worker node `deploy.sh` written and bootstrapped → M7_003_PROD_ACCEPTANCE.md

---

## 6.0 Out of Scope

- New runtime feature development unrelated to deployment execution
- Provider migrations outside current v1 deployment contract
