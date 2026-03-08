# M5_003: Enforce Workspace Entitlements And Plan Limits

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 003
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P0 — policy safety gate
**Batch:** B3 — needs M5_002
**Depends on:** M5_002 (Operate Multi-Tenant Harness Control Plane)

---

## 1.0 Singular Function

**Status:** PENDING

Implement one working policy function: deterministic per-workspace entitlement enforcement at compile/activate boundaries.

**Dimensions:**
- 1.1 PENDING Define plan-tier policy model (`Free`, `Pro`, `Team`, `Enterprise`)
- 1.2 PENDING Define entitlement source-of-truth and fail-closed defaults
- 1.3 PENDING Enforce skill/profile/usage limit checks in compile and activate flows
- 1.4 PENDING Emit machine-readable rejection reasons and policy audit snapshots

---

## 2.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Unit test: disallowed skill is rejected with stable reason code
- 2.2 PENDING Unit test: over-limit stage/profile is rejected deterministically
- 2.3 PENDING Integration test: entitlement snapshot is persisted with policy decision

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Every workspace resolves to deterministic entitlement policy
- [ ] 3.2 Compile/activate fails closed on policy violations
- [ ] 3.3 Operators receive actionable, machine-readable enforcement errors
- [ ] 3.4 Demo evidence captured for allow/deny entitlement scenarios

---

## 4.0 Out of Scope

- External payment-provider coupling
- Customer billing portal UX
