# M7_003: Free Plan Exhaustion Interruption And Conversion UX

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 003
**Date:** Mar 13, 2026
**Status:** PENDING
**Priority:** P0 — align runtime stop behavior and user-visible conversion contract
**Batch:** B1 — after M6_002 backend credit lifecycle is stable
**Depends on:** M6_002 (Free Plan $10 Credit Lifecycle)

---

## 1.0 Active Run Exhaustion Contract

**Status:** PENDING

Define whether Free-plan runs are interrupted in-flight when credit reaches zero and implement the terminal-state contract accordingly.

**Dimensions:**
- 1.1 PENDING Decide whether debit is checked only at run finalization or also during active execution
- 1.2 PENDING If mid-run stop is required, emit deterministic terminal state and reason code at exhaustion
- 1.3 PENDING Preserve idempotent billing/credit accounting when interruption and retries interact

---

## 2.0 CLI And Website UX Contract

**Status:** PENDING

Define the user-visible behavior that CLI and website pricing copy must reflect exactly.

**Dimensions:**
- 2.1 PENDING CLI output includes remaining credit balance and clear exhaustion message
- 2.2 PENDING Website pricing page references exact $10 free credit with no expiry
- 2.3 PENDING Exhausted Free plan to Scale upgrade handoff is single-step and operator-visible
- 2.4 PENDING No insecure fallback auth or billing path is introduced

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Runtime interruption policy is explicit and matches implementation exactly
- [ ] 3.2 Exhaustion message and upgrade path match backend behavior across CLI and website
- [ ] 3.3 Demo evidence captured for exhausted run behavior and conversion handoff

---

## 4.0 Out of Scope

- Changing the completed-runtime debit model shipped in M6_002
- Credit top-up or renewal on Free plan
- New payment-provider integrations beyond existing Scale upgrade path
