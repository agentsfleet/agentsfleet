# M5_003: Enforce Workspace Entitlements And Plan Limits

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 003
**Date:** Mar 06, 2026
**Status:** DONE
**Priority:** P0 — policy safety gate
**Batch:** B3 — needs M5_002
**Depends on:** M5_002 (Operate Multi-Tenant Harness Control Plane)

---

## 1.0 Singular Function

**Status:** DONE

Implement one working policy function: deterministic per-workspace entitlement enforcement at compile/activate boundaries.

**Dimensions:**
- 1.1 DONE Define plan-tier policy model (`Free`, `Scale`)
- 1.2 DONE Define entitlement source-of-truth and Clerk Management API credential lifecycle provisioning contract (issue, rotate, revoke) with fail-closed defaults
- 1.3 DONE Enforce skill/profile/usage limit checks in compile and activate flows
- 1.4 DONE Emit machine-readable rejection reasons and policy audit snapshots

---

## 2.0 Verification Units

**Status:** DONE

**Dimensions:**
- 2.1 DONE Unit test: disallowed skill is rejected with stable reason code
- 2.2 DONE Unit test: over-limit stage/profile is rejected deterministically
- 2.3 DONE Integration test: entitlement snapshot is persisted with policy decision

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Every workspace resolves to deterministic entitlement policy
- [x] 3.2 Compile/activate fails closed on policy violations
- [x] 3.3 Operators receive actionable, machine-readable enforcement errors
- [x] 3.4 Demo evidence captured for allow/deny entitlement scenarios

---

## 4.0 Out of Scope

- External payment-provider coupling
- Customer billing portal UX

---

## 5.0 Implementation Notes (Mar 13, 2026)

- Added entitlement source-of-truth schema migration: `workspace_entitlements` plus `entitlement_policy_audit_snapshots`.
- Added compile/activate enforcement path with fail-closed behavior when no entitlement row exists.
- Added automatic `FREE` entitlement provisioning on workspace creation paths.
- Added machine-readable entitlement rejection codes in `src/errors/codes.zig` and mapped them into HTTP error responses.
- Added policy audit snapshot persistence at both compile and activate boundaries.
- UUID requirement applied to new entitlement IDs (`entitlement_id`, `snapshot_id`) using PostgreSQL `UUID` column types.

---

## 6.0 Oracle Review (Mar 13, 2026: 07:59 AM)

- Attempted Oracle review run with `claude-4.6-sonnet` and `gpt-5.3`.
- `claude-4.6-sonnet` failed with transport parse error (`Unexpected token '<'` from upstream response path).
- `gpt-5.3` failed due invalid API key in environment.
- Result: Oracle review execution blocked by runtime auth/transport configuration; implementation verified with `zig build test` and `make lint`.
