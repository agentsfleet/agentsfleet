# M7_001: Deploy From Deployment Guide (DEV And PROD)

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 001
**Date:** Mar 08, 2026
**Updated:** Mar 19, 2026: 03:30 PM
**Status:** IN_PROGRESS
**Priority:** P0 — deployment execution gate
**Depends on:** M6_006 (Validate v1 Acceptance E2E Gate), M4_007 (Runtime Environment Contract), M7_002 (Documentation Production And Publish)

---

## 1.0 DEV Deployment Execution

**Status:** IN_PROGRESS

Execute deployment exactly from `docs/DEPLOYMENT.md` for DEV and record deterministic evidence.

**Dimensions:**
- 1.1 ✅ DONE Gitleaks wired as hard prerequisite in release DAG (`docker` job now `needs: [verify-tag, binaries]`; gitleaks runs on all PR/push lanes). `.gitleaks.toml` updated to ignore `.tmp/`, `zig-cache/`, `.zig-cache/`, `zig-out/` paths.
- 1.2 PENDING Must implement DEV data-plane provisioning automation from `docs/DEPLOYMENT.md` (DB roles/migrations, Redis stream bootstrap via `XGROUP CREATE`)
- 1.3 IN_PROGRESS Container deployment path unblocked: `Dockerfile` refactored to binary-copy model; `deploy-dev.yml` wired (main push → GHCR → Railway DEV → /healthz poll → QA smoke → Discord notify); `release.yml` deploy stubs remain — Railway deploy hook wiring is next.
- 1.4 IN_PROGRESS `deploy-dev.yml` has `/healthz` + `/readyz` verify-dev job and Playwright QA smoke step. Full evidence artifact (logs, `zombied doctor`, acceptance flow) not yet captured.

---

## 2.0 PROD Deployment Execution

**Status:** IN_PROGRESS

Execute production deployment using the same guide with production env separation and safety checks.

**Dimensions:**
- 2.1 PENDING Must implement PROD data-plane provisioning and network controls exactly per guide (Cloudflare/LB/Tailscale/allowlists)
- 2.2 PENDING Must replace `deploy-prod` stubs in `.github/workflows/release.yml` with real API + worker deploy, preserving role-separated DB/Redis contracts
- 2.3 PENDING Must add PROD runtime verification for HTTPS ingress at LB and private upstream `zombied serve` behavior
- 2.4 PENDING Must run production smoke checks and publish a reproducible PROD evidence package in CI artifacts

---

## 3.0 Deployment Guide Validation

**Status:** IN_PROGRESS

Treat deployment execution as validation of `docs/DEPLOYMENT.md` correctness and completeness.

**Dimensions:**
- 3.1 PENDING Must log every ambiguous/manual guess step during DEV/PROD rollout (command, context, correction)
- 3.2 PENDING Must patch `docs/DEPLOYMENT.md` for each observed drift/ambiguity before signoff
- 3.3 PENDING Must re-run all changed guide steps to verify documentation fixes are executable
- 3.4 PENDING Must freeze and tag the deployment-guide revision used for release execution

---

## 4.0 Acceptance Criteria

**Status:** IN_PROGRESS

- [ ] 4.1 DEV deployment succeeds from docs alone
- [ ] 4.2 PROD deployment succeeds from docs alone
- [ ] 4.3 Deployment docs are corrected for all execution drift found
- [ ] 4.4 Evidence artifact is complete for both DEV and PROD

---

## 6.0 Pending Implementation Notes (Must Implement)

- ✅ Wire `gitleaks` as a hard prerequisite in the tag release/deploy DAG — done via `needs: [verify-tag, binaries]` on `docker` job
- ✅ Container build path fixed: `Dockerfile` now copies pre-built binary (`dist/zombied-linux-${TARGETARCH}`) instead of running `zig build` inside Docker. `.dockerignore` added to keep build context minimal.
- ✅ `docker-compose.yml` — local dev: Postgres 18 + Redis 7 with inline TLS cert generation at startup (no mounted config files). Worker naming standardised to `zombie-worker-{animal}`.
- ✅ `scripts/check-credentials.sh` — portable credential gate for all vault items; DEV vault fully green, PROD missing 3 items (`planetscale-prod`, `tailscale/authkey`, `worker-ssh/private-key`).
- ✅ Playbooks (M1_001, M2_001, M2_002) — vault references abstracted to `$VAULT_DEV`/`$VAULT_PROD`; human/agent split documented for Tailscale + worker SSH; redis-acl-* items removed (Upstash manages ACL via dashboard).
- ✅ `schema/redis-bootstrap.sh` deleted — replaced by one-liner `XGROUP CREATE` in M2_002.
- Replace `deploy-dev`/`deploy-prod` echo stubs in `release.yml` with real Railway deploy hook + OVHCloud Tailscale SSH steps
- Add deterministic post-deploy validation jobs for DEV and PROD with machine-readable outputs and artifact upload
- Generate and store DEV+PROD evidence bundles (commands, logs, health snapshots, acceptance-flow proof) as release-linked artifacts
- Finalize guide drift fixes in `docs/DEPLOYMENT.md`, then re-run and freeze the validated revision

---

## 5.0 Out of Scope

- New runtime feature development unrelated to deployment execution
- Provider migrations outside current v1 deployment contract