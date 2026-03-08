# M5_004: Integrate Usage Metering And Billing Adapter Contract

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 004
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P1 — commercial automation
**Batch:** B3 — needs M5_003
**Depends on:** M5_003 (Enforce Workspace Entitlements And Plan Limits)

---

## 1.0 Singular Function

**Status:** PENDING

Implement one working commercial function: replay-safe usage ledger and provider-agnostic billing adapter contract.

**Dimensions:**
- 1.1 PENDING Define billable units and immutable usage ledger schema
- 1.2 PENDING Emit and aggregate deterministic usage events from runtime lifecycle
- 1.3 PENDING Define adapter interface (`Noop`, `Manual`, provider-specific)
- 1.4 PENDING Define adapter outage behavior, retry/idempotency, and secure credential handling

---

## 2.0 Verification Units

**Status:** PENDING

**Dimensions:**
- 2.1 PENDING Unit test: ledger replay yields identical totals
- 2.2 PENDING Unit test: duplicate events do not double-charge
- 2.3 PENDING Integration test: adapter outage preserves accounting state and retries safely

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Usage ledger is deterministic and replay-safe
- [ ] 3.2 Entitlement sync can consume billing state without manual edits
- [ ] 3.3 Adapter outages do not corrupt usage or enforcement decisions
- [ ] 3.4 Demo evidence captured for metering/replay and adapter-failure path

---

## 4.0 Out of Scope

- Customer-facing invoice dashboard
- Vendor-specific billing implementation lock-in
