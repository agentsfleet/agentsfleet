# M7_003: Free Plan Credit Ledger, Exhaustion, And Conversion UX

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 003
**Date:** Mar 13, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — implement deterministic free-credit accounting and user-visible exhaustion flow
**Batch:** B1 — after M6_002 pricing contract is locked
**Depends on:** M6_002 (Free Plan $10 Credit Pricing Contract), M6_001 (Paid Scale Plan)

---

## 1.0 Free Plan Credit Ledger And Enforcement

**Status:** IN_PROGRESS

Implement the backend credit ledger and enforcement contract for the Free plan.

**Dimensions:**
- 1.1 PENDING Create credit ledger contract per workspace (`initial_credit`, `consumed`, `remaining`, no expiry)
- 1.2 PENDING Bind Free plan to 1 workspace execution scope with no overdraft path
- 1.3 PENDING Gate run/sync/harness endpoints with Free plan balance checks and explicit `CREDIT_EXHAUSTED` error contract
- 1.4 PENDING Add deterministic audit/metrics events for credit grant, credit deduction, and credit exhaustion

---

## 2.0 Runtime Exhaustion Interruption Contract

**Status:** PENDING

Define whether Free-plan runs are interrupted in-flight when credit reaches zero and implement the terminal-state contract accordingly.

**Dimensions:**
- 2.1 PENDING Decide whether debit is checked only at run finalization or also during active execution
- 2.2 PENDING If mid-run stop is required, emit deterministic terminal state and reason code at exhaustion
- 2.3 PENDING Deduct credit only for completed agent runtime; failed/incomplete runs remain free
- 2.4 PENDING Preserve idempotent billing/credit accounting when interruption and retries interact

---

## 3.0 CLI And Website UX Contract

**Status:** PENDING

Define the user-visible behavior that CLI and website pricing copy must reflect exactly.

**Dimensions:**
- 3.1 PENDING CLI output includes remaining credit balance and clear exhaustion message
- 3.2 PENDING Website pricing page references exact $10 free credit with no expiry
- 3.3 PENDING Exhausted Free plan to Scale upgrade handoff is single-step and operator-visible
- 3.4 PENDING No insecure fallback auth or billing path is introduced

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 New user receives $10 credit and can execute agent workloads until credit is consumed
- [ ] 4.2 At $0 balance, gated endpoints and in-flight behavior follow one explicit enforcement policy
- [ ] 4.3 Credit deductions match completed runtime only; failed runs are free
- [ ] 4.4 Exhaustion message and upgrade path match backend behavior across CLI and website
- [ ] 4.5 Demo evidence captured for ledger state, exhausted run behavior, and conversion handoff

---

## 5.0 Out of Scope

- Credit top-up or renewal on Free plan
- New payment-provider integrations beyond existing Scale upgrade path
