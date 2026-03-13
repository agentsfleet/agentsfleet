# M6_002: Free Plan $10 Credit Pricing Contract

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 002
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P0 — reduce onboarding friction with deterministic credit control
**Depends on:** M5_003 (Workspace Entitlements and Plan Limits), M6_001 (Paid Scale Plan)

---

## 1.0 Free Plan Pricing Contract

**Status:** DONE

Define the user-facing Free plan promise: every new user sees a $10 credit with no expiry in pricing and plan copy, with the implementation details delegated to follow-on work.

**Dimensions:**
- 1.1 DONE Define Free plan pricing copy as `$10 credit included (no expiry)`
- 1.2 DONE Publish Free vs Scale positioning in website pricing content
- 1.3 DONE Keep the Free plan promise explicit and deterministic in tests
- 1.4 DONE Leave ledger, exhaustion, and conversion implementation to downstream workstreams

---

## 2.0 Verification Units

**Status:** DONE

Verify the pricing contract is present and stable in the website experience.

**Dimensions:**
- 2.1 DONE Unit test: pricing page renders Free and Scale tiers
- 2.2 DONE Unit test: pricing page renders exact `$10 credit included (no expiry)` copy
- 2.3 DONE E2E test: pricing page exposes the same Free plan promise
- 2.4 DONE Navigation/smoke coverage keeps the pricing contract visible across the website

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Website pricing copy references exact $10 free credit with no expiry
- [x] 3.2 Free vs Scale plan positioning is explicit and test-covered
- [x] 3.3 Backend ledger and exhaustion behavior are intentionally deferred to follow-on work
- [x] 3.4 The public pricing contract is stable enough for downstream implementation specs to depend on

---

## 4.0 Out Of Scope

- Backend credit ledger and balance accounting
- Runtime exhaustion enforcement and stop semantics
- CLI remaining-credit output and exhaustion errors
- Free-to-Scale conversion flow after exhaustion
- Unlimited anonymous usage
- Credit top-up or renewal on Free plan in v1
