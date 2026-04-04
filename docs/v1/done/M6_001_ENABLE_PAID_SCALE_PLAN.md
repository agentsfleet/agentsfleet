# M6_001: Enable Paid Scale Plan End-to-End

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 001
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P0 — revenue gate
**Batch:** B4 — capstone revenue gate
**Depends on:** M5_003 (Enforce Workspace Entitlements And Plan Limits), M5_004 (Integrate Usage Metering And Billing Adapter Contract)

---

## 1.0 Singular Function

**Status:** DONE

Implement one working paid-plan function: the `Scale` tier operates as pay-as-you-go for agent runtime, with usage-based billing for completed agent execution and no charge for failed or incomplete runs.

**Dimensions:**
- 1.1 DONE Define `Scale` plan billing SKU and entitlement mapping (multi-repo, multiple harness playbooks, higher concurrency)
- 1.2 DONE Implement upgrade flow from `Free` -> `Scale`
- 1.3 DONE Apply `Scale` entitlement limits at compile/activate/runtime boundaries
- 1.4 DONE Implement pay-as-you-go metering: charge only for completed agent runtime, zero charge on failed/incomplete runs
- 1.5 DONE Implement downgrade/failed-payment behavior with deterministic grace handling

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit test: upgrade applies new entitlements deterministically
- 2.2 DONE Unit test: completed runs are metered; failed/incomplete runs are not charged
- 2.3 DONE Unit test: payment failure transitions to grace and then downgrade policy
- 2.4 DONE Integration test: paid workspace run path remains stable across billing sync cycles

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 One paid `Scale` plan works end-to-end from subscription to enforcement
- [x] 3.2 Free->Scale and Scale->Free transitions are deterministic and auditable
- [x] 3.3 Pay-as-you-go metering charges only for successful agent runtime
- [x] 3.4 Demo evidence captured for paid-plan lifecycle (activate, enforce, downgrade)

---

## 4.0 Out of Scope

- Multi-plan catalog rollout beyond `Scale`
- Customer self-serve billing dashboard UX

---

## 5.0 Implementation Notes (Mar 13, 2026: 12:05 PM)

- Added migration `schema/014_workspace_billing_state.sql` for `workspace_billing_state`, `workspace_billing_audit`, and runtime entitlement audit support.
- Added `src/state/workspace_billing.zig` as the billing source of truth for `Free` and `Scale`, including deterministic grace handling and downgrade reconciliation.
- Provisioned billing state on workspace creation and legacy GitHub-installation workspace upsert paths.
- Added `POST /v1/workspaces/{workspace_id}/billing/scale` to perform the `Free` -> `Scale` upgrade flow.
- Added `POST /v1/workspaces/{workspace_id}/billing/events` to record `PAYMENT_FAILED` and `DOWNGRADE_TO_FREE` lifecycle events through a production path instead of test-only SQL.
- Updated `POST /v1/workspaces/{workspace_id}:sync` to reconcile pending billing state and surface current `plan_tier`, `billing_status`, `plan_sku`, and `grace_expires_at`.
- Reconciled billing state before harness compile/activate and before run start, then re-applied entitlement enforcement at the runtime boundary against the active profile snapshot.
- Closed the metering loop so successful runs finalize as billable and blocked/failed/incomplete runs finalize as non-billable.

---

## 6.0 Verification Evidence (Mar 13, 2026: 12:05 PM)

- `zig build test` — passed after the M6 implementation updates.
- Evidence note: `docs/evidence/M6_001_PAID_SCALE_PLAN_LIFECYCLE.md`
- `src/state/workspace_billing_test.zig` covers deterministic `Free` -> `Scale` upgrade and the production billing lifecycle path for `payment_failed -> grace -> downgrade`.
- `src/state/workspace_billing_transition.zig` covers pure grace-window and downgrade decision logic.
- `src/state/billing.zig` tests cover success-only billable finalization and zero-charge failed/incomplete finalization.
- Runtime enforcement path is exercised by the shared entitlement enforcement module now invoked from compile, activate, and run start.
