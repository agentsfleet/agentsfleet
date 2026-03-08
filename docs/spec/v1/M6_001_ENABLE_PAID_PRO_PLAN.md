# M6_001: Enable Paid Pro Plan End-to-End

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 001
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P0 — revenue gate
**Batch:** B4 — capstone revenue gate
**Depends on:** M5_003 (Enforce Workspace Entitlements And Plan Limits), M5_004 (Integrate Usage Metering And Billing Adapter Contract)

---

## 1.0 Singular Function

**Status:** PENDING

Implement one working paid-plan function: a specific paid tier (`Pro`) can subscribe, activate, enforce limits, and continue operating without manual intervention.

**Dimensions:**
- 1.1 PENDING Define `Pro` plan billing SKU and entitlement mapping
- 1.2 PENDING Implement upgrade flow from `Free` -> `Pro`
- 1.3 PENDING Apply `Pro` entitlement limits at compile/activate/runtime boundaries
- 1.4 PENDING Implement downgrade/failed-payment behavior with deterministic grace handling

---

## 2.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Unit test: upgrade applies new entitlements deterministically
- 2.2 PENDING Unit test: payment failure transitions to grace and then downgrade policy
- 2.3 PENDING Integration test: paid workspace run path remains stable across billing sync cycles

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 One paid `Pro` plan works end-to-end from subscription to enforcement
- [ ] 3.2 Free->Pro and Pro->Free transitions are deterministic and auditable
- [ ] 3.3 Demo evidence captured for paid-plan lifecycle (activate, enforce, downgrade)

---

## 4.0 Out of Scope

- Multi-plan catalog rollout beyond `Pro`
- Customer self-serve billing dashboard UX
