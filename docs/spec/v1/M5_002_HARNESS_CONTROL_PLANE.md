# M5_002: Operate Multi-Tenant Harness Control Plane

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 002
**Date:** Mar 06, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — multi-tenant runtime core
**Batch:** B2 — finish tenant isolation (M4_003 already DONE)
**Depends on:** None (M4_003 already DONE)

---

## 1.0 Singular Function

**Status:** IN_PROGRESS

Implement one working control-plane function: compile, validate, activate, and resolve workspace harness profiles deterministically.

**Dimensions:**
- 1.1 DONE Data model contracts for profiles, versions, active bindings, compile jobs, and skill secrets
- 1.2 DONE API contracts for source put, compile, activate, fetch active, and skill secret lifecycle
- 1.3 DONE Runtime fallback to `default-v1` and run-snapshot pinning
- 1.4 IN_PROGRESS Tenant isolation and policy hooks for future entitlement checks

---

## 2.0 Verification Units

**Status:** IN_PROGRESS

**Dimensions:**
- 2.1 DONE Unit test: invalid profile cannot activate
- 2.2 DONE Unit test: missing active profile falls back to `default-v1`
- 2.3 PENDING Integration test: per-workspace scoping blocks cross-tenant access
- 2.4 PENDING Integration test: compile and activation history is auditable and immutable

---

## 3.0 Acceptance Criteria

**Status:** IN_PROGRESS

- [x] 3.1 Harness profiles compile into executable graph JSON
- [x] 3.2 Runtime resolves and executes active profile deterministically
- [ ] 3.3 Tenant isolation is fully enforced and test-backed
- [ ] 3.4 Demo evidence captured for compile -> activate -> run path

---

## 4.0 Out of Scope

- Entitlement policy matrix (tracked in M5_003)
- Usage billing integration (tracked in M5_004)
