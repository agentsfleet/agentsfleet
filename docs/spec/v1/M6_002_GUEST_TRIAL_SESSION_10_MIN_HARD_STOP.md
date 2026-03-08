# M6_002: Guest Trial Session 10-Minute Hard Stop

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 002
**Date:** Mar 06, 2026
**Status:** PENDING
**Priority:** P0 — reduce trial friction with strict abuse control
**Depends on:** M5_003 (Workspace Entitlements and Plan Limits), M6_001 (Paid Pro Plan)

---

## 1.0 Guest Trial Session Contract

**Status:** PENDING

Define a guest session model for users who are not logged in, with deterministic lifecycle and strict 10-minute maximum runtime.

**Dimensions:**
- 1.1 PENDING Create guest session token contract (issued_at, expires_at, trial_scope, request_id)
- 1.2 PENDING Enforce absolute `expires_at = issued_at + 10 minutes` with no extension path
- 1.3 PENDING Bind guest session to one ephemeral workspace execution scope
- 1.4 PENDING Return explicit error contract on expiration (`TRIAL_EXPIRED`, retry path to signup/login)

---

## 2.0 Runtime Enforcement And Cleanup

**Status:** PENDING

Implement backend enforcement so guest work cannot continue after the 10-minute wall-clock limit.

**Dimensions:**
- 2.1 PENDING Gate all run/sync/harness endpoints with guest trial entitlement check
- 2.2 PENDING Hard-stop active guest runs at expiry and emit terminal state + reason code
- 2.3 PENDING Auto-cleanup ephemeral workspace artifacts and secrets after hard stop
- 2.4 PENDING Add deterministic audit/metrics events for trial start, trial stop, and forced expiry

---

## 3.0 CLI And Website UX Contract (Pre-Pricing Update)

**Status:** PENDING

Define the user-visible behavior that website pricing copy and CLI messaging must reflect.

**Dimensions:**
- 3.1 PENDING CLI output includes remaining trial time and clear expiration message
- 3.2 PENDING Website pricing page references exact 10-minute guest trial limit (no ambiguous wording)
- 3.3 PENDING Conversion handoff path from expired guest session to signup/login is single-step
- 3.4 PENDING No insecure fallback auth path is introduced for guest or paid flows

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Not-logged-in user can start guest trial and execute workload for up to 10 minutes
- [ ] 4.2 At 10 minutes, in-flight guest activity is forcibly stopped with deterministic error contract
- [ ] 4.3 Guest artifacts are cleaned up and cannot be reused after expiry
- [ ] 4.4 Pricing and CLI copy match enforced backend behavior exactly
- [ ] 4.5 Demo evidence captured (API logs, CLI output, and website screenshot copy)

---

## 5.0 Out Of Scope

- Unlimited anonymous usage
- Guest trial renewal/extension in v1
- Credit card capture before first guest trial run
