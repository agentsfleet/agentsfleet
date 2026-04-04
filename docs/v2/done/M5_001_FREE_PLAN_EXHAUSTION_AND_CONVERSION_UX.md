# M5_001: Free Plan Credit Ledger, Exhaustion, And Conversion UX

**Prototype:** v2.0.0
**Milestone:** M5
**Workstream:** 001
**Date:** Mar 23, 2026
**Status:** DONE
**Priority:** P2 — post-v1 monetization hardening; deferred from v1.0 launch gate
**Batch:** B1 — first v2 monetization batch
**Depends on:** v1/M12_002 (Zero-Trust DB Schema Segmentation), v1/M6_006 (Validate v1 Acceptance E2E Gate), v1/M6_002 (Free Plan $10 Credit Pricing Contract), v1/M6_001 (Paid Scale Plan)

**v1.0 Scope Decision (Mar 16, 2026):** Deferred from v1.0 release gating. This workstream remains important for revenue protection and free-tier abuse control, but is not required to pass v1.0 acceptance.

---

## 1.0 Free Plan Credit Ledger And Enforcement

**Status:** DONE

Implement the backend credit ledger and enforcement contract for the Free plan.

**Dimensions:**
- 1.1 DONE Create credit ledger contract per workspace (`initial_credit`, `consumed`, `remaining`, no expiry)
- 1.2 DONE Bind Free plan to 1 workspace execution scope with no overdraft path
- 1.3 DONE Gate run/sync/harness endpoints with Free plan balance checks and explicit `CREDIT_EXHAUSTED` error contract
- 1.4 DONE Add deterministic audit/metrics events for credit grant, credit deduction, and credit exhaustion

---

## 2.0 Runtime Exhaustion Interruption Contract

**Status:** DONE

Define whether Free-plan runs are interrupted in-flight when credit reaches zero and implement the terminal-state contract accordingly.

**Dimensions:**
- 2.1 DONE Decide whether debit is checked only at run finalization or also during active execution
- 2.2 DONE Mid-run stop is not used; exhaustion is enforced at admission and surfaced on the next execution attempt
- 2.3 DONE Deduct credit only for completed agent runtime; failed/incomplete runs remain free
- 2.4 DONE Preserve idempotent billing/credit accounting when interruption and retries interact

---

## 3.0 CLI And Website UX Contract

**Status:** DONE

Define the user-visible behavior that CLI and website pricing copy must reflect exactly.

**Dimensions:**
- 3.1 DONE CLI output includes remaining credit balance and clear exhaustion message
- 3.2 DONE Website pricing page references exact $10 free credit with no expiry
- 3.3 DONE Exhausted Free plan to Scale upgrade handoff is single-step and operator-visible
- 3.4 DONE No insecure fallback auth or billing path is introduced

---

## 3.1 Operator Control For Scoring Context Cap

**Status:** DONE

Add explicit operator control for scoring context token cap to support abuse control and large-repo tuning.

**Dimensions:**
- 3.1.1 DONE Add `zombiectl admin config set scoring_context_max_tokens <n>` with bounds validation (512-8192) and deterministic error messaging

---

## 3.2 Role-Based Access Control For Operator And Admin Commands

**Status:** DONE

**Audit Finding (Mar 16, 2026):** The server `AuthPrincipal` carries `user_id`, `tenant_id`, and `workspace_scope_id` but no `role` claim. Every authenticated user can hit every endpoint including operator-level surfaces: `harness source put/compile/activate`, `skill-secret put/delete`, `agent scores/profile`, and future `admin config` commands. There is no RBAC fence between workspace users and workspace operators.

**Dimensions:**
- 3.2.1 DONE Add `role` claim to JWT via Clerk custom claims (values: `user`, `operator`, `admin`)
- 3.2.2 DONE Add server-side RBAC middleware that checks role before allowing harness, skill-secret, agent, and admin endpoints; return `403 FORBIDDEN` with deterministic error code `INSUFFICIENT_ROLE`
- 3.2.3 DONE CLI reads role from token and auto-shows/hides operator commands in `--help` output; operator commands, including `workspace upgrade-scale`, remain callable but hidden from default help for non-operator tokens
- 3.2.4 DONE Acceptance evidence: live HTTP integration tests prove non-operator token receives 403 on harness/skill-secret/admin endpoints, while operator/admin tokens pass the role fence for their allowed surfaces

**Short-term mitigation (pre-RBAC, shipped in M6_006 CLI audit):** Operator commands (`harness`, `skill-secret`, `agent`) hidden from default `--help` unless `ZOMBIE_OPERATOR=1` env var is set. Commands still functional for any authenticated user until server-side RBAC lands.

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 New user receives $10 credit and can execute agent workloads until credit is consumed
- [x] 4.2 At $0 balance, gated endpoints and in-flight behavior follow one explicit enforcement policy
- [x] 4.3 Credit deductions match completed runtime only; failed runs are free
- [x] 4.4 Exhaustion message and upgrade path match backend behavior across CLI and website
- [x] 4.5 Demo evidence captured for ledger state, exhausted run behavior, conversion handoff, and RBAC route enforcement

---

## 5.0 Out of Scope

- Credit top-up or renewal on Free plan
- New payment-provider integrations beyond existing Scale upgrade path
- v1.0 go/no-go release decision (owned by M6_006 acceptance gate)
