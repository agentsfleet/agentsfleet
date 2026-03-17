# M7_001: Deploy From Deployment Guide (DEV And PROD)

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 001
**Date:** Mar 08, 2026
**Updated:** Mar 15, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — deployment execution gate
**Depends on:** M6_006 (Validate v1 Acceptance E2E Gate), M4_007 (Runtime Environment Contract), M7_002 (Documentation Production And Publish)

---

## 1.0 DEV Deployment Execution

**Status:** IN_PROGRESS

Execute deployment exactly from `docs/DEPLOYMENT.md` for DEV and record deterministic evidence.

**Dimensions:**
- 1.1 ✅ DONE Gitleaks workflow exists and is enforced on PR/push lanes; must still be wired as the first gate in the release/deploy DAG before deploy jobs
- 1.2 PENDING Must implement DEV data-plane provisioning automation from `docs/DEPLOYMENT.md` (DB roles/migrations, Redis ACL/stream contracts, runtime prerequisites)
- 1.3 PENDING Must replace `deploy-dev` stub in `.github/workflows/release.yml` with real `zombied serve` + `zombied worker` deployment steps
- 1.4 PENDING Must add post-deploy DEV verification gate: `/healthz`, `/readyz`, `zombied doctor`, and acceptance flow (`login` -> `workspace add` -> `run`) with captured logs

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

- Wire `gitleaks` as a hard prerequisite in the tag release/deploy DAG, not only as a separate PR/push workflow
- Replace `deploy-dev`/`deploy-prod` echo stubs with real SSH or platform-native deploy steps using 1Password-loaded secrets
- Add deterministic post-deploy validation jobs for DEV and PROD with machine-readable outputs and artifact upload
- Generate and store DEV+PROD evidence bundles (commands, logs, health snapshots, acceptance-flow proof) as release-linked artifacts
- Finalize guide drift fixes in `docs/DEPLOYMENT.md`, then re-run and freeze the validated revision

---

## 5.0 Out of Scope

- New runtime feature development unrelated to deployment execution
- Provider migrations outside current v1 deployment contract
