# M6_002: Free Plan $10 Credit Lifecycle

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 002
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P0 — reduce onboarding friction with deterministic credit control
**Depends on:** M5_003 (Workspace Entitlements and Plan Limits), M6_001 (Paid Scale Plan)

---

## 1.0 Free Plan Credit Contract

**Status:** PENDING

Define the Free plan credit model: every new user receives a $10 credit (no expiry) to run agents, with deterministic enforcement when credit is exhausted.

**Dimensions:**
- 1.1 PENDING Create credit ledger contract per workspace (initial_credit, consumed, remaining, no expiry)
- 1.2 PENDING Enforce hard stop when credit reaches $0 with no overdraft path
- 1.3 PENDING Bind Free plan to 1 workspace execution scope
- 1.4 PENDING Return explicit error contract on credit exhaustion (`CREDIT_EXHAUSTED`, upgrade path to Scale)

---

## 2.0 Runtime Enforcement And Metering

**Status:** PENDING

Implement backend enforcement so Free plan work cannot continue after the $10 credit is consumed.

**Dimensions:**
- 2.1 PENDING Gate all run/sync/harness endpoints with Free plan credit balance check
- 2.2 PENDING Hard-stop active runs at credit exhaustion and emit terminal state + reason code
- 2.3 PENDING Deduct credit only for completed agent runtime (match Scale metering: no charge for failed/incomplete runs)
- 2.4 PENDING Add deterministic audit/metrics events for credit grant, credit deduction, and credit exhaustion

---

## 3.0 CLI And Website UX Contract

**Status:** PENDING

Define the user-visible behavior that website pricing copy and CLI messaging must reflect.

**Dimensions:**
- 3.1 PENDING CLI output includes remaining credit balance and clear exhaustion message
- 3.2 PENDING Website pricing page references exact $10 free credit with no expiry (no ambiguous wording)
- 3.3 PENDING Conversion handoff path from exhausted Free plan to Scale upgrade is single-step
- 3.4 PENDING No insecure fallback auth path is introduced for Free or Scale flows

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 New user receives $10 credit and can execute agent workloads until credit is consumed
- [ ] 4.2 At $0 balance, in-flight activity is stopped with deterministic error contract
- [ ] 4.3 Credit deductions match completed runtime only (failed runs are free)
- [ ] 4.4 Pricing and CLI copy match enforced backend behavior exactly
- [ ] 4.5 Demo evidence captured (API logs, CLI output, and website screenshot copy)

---

## 5.0 Out Of Scope

- Unlimited anonymous usage
- Credit top-up or renewal on Free plan in v1
- Credit card capture before first Free plan run
