# M5_002: Operate Multi-Tenant Harness Control Plane

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 002
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P0 — multi-tenant runtime core
**Batch:** B2 — finish tenant isolation (M4_003 already DONE)
**Depends on:** None (M4_003 already DONE)

---

## 1.0 Singular Function

**Status:** DONE

Implement one working control-plane function: compile, validate, activate, and resolve workspace harness profiles deterministically.

**Dimensions:**
- 1.1 DONE Data model contracts for profiles, versions, active bindings, compile jobs, and skill secrets
- 1.2 DONE API contracts for source put, compile, activate, fetch active, and skill secret lifecycle
- 1.3 DONE Runtime fallback to `default-v1` and run-snapshot pinning
- 1.4 DONE Tenant isolation and policy hooks for future entitlement checks

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit test: invalid profile cannot activate
- 2.2 DONE Unit test: missing active profile falls back to `default-v1`
- 2.3 DONE Integration test: per-workspace scoping blocks cross-tenant access
- 2.4 DONE Integration test: compile and activation history is auditable and immutable

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Harness profiles compile into executable graph JSON
- [x] 3.2 Runtime resolves and executes active profile deterministically
- [x] 3.3 Tenant isolation is fully enforced and test-backed
- [x] 3.4 Demo evidence captured for compile -> activate -> run path

---

## 4.0 Out of Scope

- Entitlement policy matrix (tracked in M5_003)
- Usage billing integration (tracked in M5_004)

---

## 5.0 Auth Decision Log

**Status:** DONE

- 5.1 DONE Industry-standard auth baseline: Clerk-issued JWTs are the canonical auth path for API access in production.
- 5.2 DONE Inbound integrations (Slack/Linear/GitHub webhooks) must use provider signature verification with replay protection, not API keys.
- 5.3 DONE App-managed `api_keys` storage is excluded from M5_002 final design; tenant/workspace scope enforcement is driven by JWT claims plus RLS session context.
- 5.4 DONE Clerk M2M `jwt_oidc` claim enforcement in `zombied`: when `workspace_id` claim is present, cross-workspace access is denied even within the same tenant.
