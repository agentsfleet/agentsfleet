# M7_002: Documentation Production And Publish

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 002
**Date:** Mar 08, 2026
**Status:** PENDING
**Priority:** P0 — documentation release gate
**Depends on:** M6_004 (Marketing CTA Lead Capture Evaluation), M6_005 (GitHub CI and Release Pipeline), M4_007 (Runtime Environment Contract)

---

## 1.0 Command And Install Documentation

**Status:** PENDING

Publish canonical command and install docs for both binaries and public install surfaces.

**Dimensions:**
- 1.1 PENDING Add `docs/ZOMBIED.md` with `serve`, `worker`, `doctor`, and `migrate`
- 1.2 PENDING Add `docs/ZOMBIECTL.md` with install/auth/workspace/specs/run commands
- 1.3 PENDING Document machine-readable flags (`--format=json` / `--json`) and exit behavior
- 1.4 PENDING Document deterministic config precedence (`CLI flag > env > .env.local dev fallback > default`) and install paths (`curl https://usezombie.sh/install.sh`, npm, npx) with verified examples

---

## 2.0 Deployment And Architecture Documentation

**Status:** PENDING

Publish deployment and architecture docs aligned with split API/worker runtime.

**Dimensions:**
- 2.1 PENDING Align `docs/ARCHITECTURE.md` with API-only `serve` and isolated `worker` processes
- 2.2 PENDING Align `docs/DEPLOYMENT.md` with HTTPS-at-LB/private upstream model
- 2.3 PENDING Align runtime env docs with role-separated DB/Redis and `rediss://` contract
- 2.4 PENDING Add explicit operator runbooks for `zombied doctor` and `zombied doctor worker`

---

## 3.0 Marketing And Analytics Documentation

**Status:** PENDING

Publish operator-ready docs for CTA capture behavior, analytics, and consent text.

**Dimensions:**
- 3.1 PENDING Document human CTA flow and success behavior (`Notify me`)
- 3.2 PENDING Document agent-route isolation from human lead pipeline
- 3.3 PENDING Document provider/list env setup and rollback procedure
- 3.4 PENDING Document CTA analytics events, UTM fields, and consent/retention notes

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 CLI/install docs are complete and match shipped behavior
- [ ] 4.2 Deployment/architecture docs are complete and match shipped behavior
- [ ] 4.3 Marketing/analytics docs are complete and operator-actionable
- [ ] 4.4 Docs are publish-ready for docs portal and release notes

---

## 5.0 Out of Scope

- New feature development unrelated to documentation parity
- Website redesign beyond documentation correctness
