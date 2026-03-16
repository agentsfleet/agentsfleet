# M12_001: Free Plan Credit Ledger, Exhaustion, And Conversion UX

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 1
**Date:** Mar 16, 2026
**Status:** PENDING
**Priority:** P2 — post-v1 monetization hardening; not a v1.0 launch gate
**Batch:** B4 — after v1.0 acceptance gate and production stabilization
**Depends on:** M11_001 (Grafana Observability Pipeline And Langfuse Async Delivery), M6_006 (Validate v1 Acceptance E2E Gate), M6_002 (Free Plan $10 Credit Pricing Contract), M6_001 (Paid Scale Plan)

**v1.0 Scope Decision (Mar 16, 2026):** Deferred from v1.0 release gating. This workstream remains important for revenue protection and free-tier abuse control, but is not required to pass v1.0 acceptance.

---

## 1.0 Free Plan Credit Ledger And Enforcement

**Status:** PENDING

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

## 3.1 Operator Control For Scoring Context Cap

**Status:** PENDING

Add explicit operator control for scoring context token cap to support abuse control and large-repo tuning.

**Dimensions:**
- 3.1.1 PENDING Add `zombiectl admin config set scoring_context_max_tokens <n>` with bounds validation (512-8192) and deterministic error messaging

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
- v1.0 go/no-go release decision (owned by M6_006 acceptance gate)
