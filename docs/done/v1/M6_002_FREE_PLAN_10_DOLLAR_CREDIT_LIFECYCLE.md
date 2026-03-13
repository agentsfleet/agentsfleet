# M6_002: Free Plan $10 Credit Lifecycle

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 002
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P0 — reduce onboarding friction with deterministic credit control
**Depends on:** M5_003 (Workspace Entitlements and Plan Limits), M6_001 (Paid Scale Plan)

---

## 1.0 Free Plan Credit Contract

**Status:** DONE

Define the backend Free plan credit model: every new workspace receives a $10 credit (no expiry) to run agents, with deterministic enforcement when credit is exhausted and a single-workspace Free creation limit per tenant.

**Dimensions:**
- 1.1 DONE Create credit ledger contract per workspace (initial_credit, consumed, remaining, no expiry)
- 1.2 DONE Block future execution when credit reaches $0 with no overdraft path
- 1.3 DONE Bind Free plan to 1 workspace creation scope per tenant unless an existing workspace is upgraded to Scale
- 1.4 DONE Return explicit error contract on credit exhaustion (`CREDIT_EXHAUSTED`, upgrade path to Scale)

---

## 2.0 Runtime Enforcement And Metering

**Status:** DONE

Implement backend enforcement so Free plan work cannot continue after the $10 credit is consumed, with debit derived from completed runtime metering.

**Dimensions:**
- 2.1 DONE Gate all run/sync/harness endpoints with Free plan credit balance check
- 2.2 DONE Deduct credit only for completed agent runtime (match Scale metering: no charge for failed/incomplete runs)
- 2.3 DONE Enforce exhausted credit from actual runtime depletion, not just preexisting zero balance
- 2.4 DONE Add deterministic audit events for credit grant, credit deduction, and credit exhaustion

---

## 3.0 Acceptance Criteria

**Status:** DONE

-- [x] 3.1 New workspace receives $10 credit and can execute agent workloads until credit is consumed
- [x] 3.2 At $0 balance, future compile/activate/run/sync execution is blocked with deterministic error contract
- [x] 3.3 Credit deductions match completed runtime only (failed runs are free)
- [x] 3.4 Free-plan tenants cannot create a second non-Scale workspace
- [x] 3.5 Deterministic audit evidence exists for credit grant, deduction, and exhaustion

---

## 4.0 Out Of Scope

- Unlimited anonymous usage
- Credit top-up or renewal on Free plan in v1
- Credit card capture before first Free plan run
- Mid-run interruption when balance reaches zero
- CLI/UI conversion and pricing copy alignment

---

## 5.0 Follow-On Scope

Deferred to [M7_003_FREE_PLAN_EXHAUSTION_AND_CONVERSION_UX.md](/Users/kishore/Projects/usezombie/docs/spec/v1/M7_003_FREE_PLAN_EXHAUSTION_AND_CONVERSION_UX.md):

- Hard-stop active runs at credit exhaustion and emit terminal state + reason code
- CLI output for remaining credit and exhaustion messaging
- Website pricing copy alignment for exact $10 no-expiry contract
- Upgrade/conversion handoff path from exhausted Free plan to Scale

---

## 6.0 Implementation Notes (Mar 13, 2026)

- Added `workspace_credit_state` and `workspace_credit_audit` ledger tables and wired them into startup migrations.
- Added explicit credit exhaustion error code `UZ-BILLING-005`.
- Provisioned Free credit during workspace creation and GitHub App callback workspace bootstrap.
- Gated Free execution on run start, sync, compile, activate, and workspace status endpoints.
- Debit now happens from completed runtime metering only, using the finalized `usage_ledger` billable quantity path.
- Added deterministic audit rows for `CREDIT_GRANTED`, `CREDIT_DEDUCTED`, and `CREDIT_EXHAUSTED`.
- Enforced single-workspace Free creation limit per tenant unless the existing workspace is already on Scale.
