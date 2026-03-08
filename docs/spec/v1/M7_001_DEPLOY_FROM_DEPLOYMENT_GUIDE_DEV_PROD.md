# M7_001: Deploy From Deployment Guide (DEV And PROD)

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 001
**Date:** Mar 08, 2026
**Status:** PENDING
**Priority:** P0 — deployment execution gate
**Depends on:** M6_006 (Validate v1 Acceptance E2E Gate), M4_007 (Runtime Environment Contract), M7_002 (Documentation Production And Publish)

---

## 1.0 DEV Deployment Execution

**Status:** PENDING

Execute deployment exactly from `docs/DEPLOYMENT.md` for DEV and record deterministic evidence.

**Dimensions:**
- 1.1 PENDING Run Gitleaks as the first CI gate (`gitleaks` job) and fail fast before any deploy jobs
- 1.2 PENDING Provision DEV data plane and runtime prerequisites per guide
- 1.3 PENDING Deploy `zombied serve` and `zombied worker` in DEV using guide sequence
- 1.4 PENDING Run health/readiness/doctor checks and one acceptance flow (`login` -> `workspace add` -> `run`)

---

## 2.0 PROD Deployment Execution

**Status:** PENDING

Execute production deployment using the same guide with production env separation and safety checks.

**Dimensions:**
- 2.1 PENDING Provision PROD data plane and network controls per guide
- 2.2 PENDING Deploy API/worker in PROD with role-separated DB/Redis contracts
- 2.3 PENDING Verify LB HTTPS ingress and private upstream runtime behavior
- 2.4 PENDING Run smoke checks and capture production evidence package

---

## 3.0 Deployment Guide Validation

**Status:** PENDING

Treat deployment execution as validation of `docs/DEPLOYMENT.md` correctness and completeness.

**Dimensions:**
- 3.1 PENDING Record every command path that required operator guesswork
- 3.2 PENDING Patch deployment docs for every observed ambiguity/drift
- 3.3 PENDING Re-run changed guide steps to confirm fixes are correct
- 3.4 PENDING Freeze deployment guide revision for release usage

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 DEV deployment succeeds from docs alone
- [ ] 4.2 PROD deployment succeeds from docs alone
- [ ] 4.3 Deployment docs are corrected for all execution drift found
- [ ] 4.4 Evidence artifact is complete for both DEV and PROD

---

## 5.0 Out of Scope

- New runtime feature development unrelated to deployment execution
- Provider migrations outside current v1 deployment contract
