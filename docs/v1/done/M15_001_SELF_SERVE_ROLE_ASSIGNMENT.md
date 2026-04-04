# M15_001: Self-Serve Role Assignment

**Prototype:** v1.0.0
**Milestone:** M15
**Workstream:** 001
**Date:** Mar 28, 2026
**Status:** DONE
**Priority:** P0 — New signups land as read-only `user`, hitting 403 on every mutation; onboarding is dead on arrival
**Batch:** B1
**Depends on:** M3_006 (Clerk Auth)

---

## 1.0 Workspace Creator Auto-Promotion

**Status:** DONE

When a user creates a workspace they are the tenant owner and must have at least `operator` access immediately. Currently the Clerk JWT carries no role claim (or defaults to `user`), so every mutation handler returns 403 on the first request after signup.

Two implementation paths are supported. Option A (Clerk API) is preferred for production. Option B (backend override) serves as a same-request fallback while the JWT has not yet been re-issued.

**Option A — Clerk API call on workspace creation (preferred):**
Call the Clerk API from the workspace creation handler to set `publicMetadata.role = "operator"` on the creating user. The next JWT re-issue carries the role.

**Option B — Backend workspace-owner override:**
If the JWT role claim is `user` AND `principal.user_id == workspace.created_by`, the auth guard upgrades the effective role to `operator` for that request without waiting for a new JWT.

**Dimensions:**
- 1.1 DONE Option A: add Clerk SDK call in workspace creation handler to set `publicMetadata.role = "operator"`
- 1.2 DONE Option A: handle Clerk API failure gracefully — workspace is still created; role assignment is retried asynchronously; no 500 surfaced to the caller
- 1.3 DONE Option A/B: idempotent guard — if user already holds `operator` or `admin`, do not downgrade
- 1.4 DONE Option B: add `workspace_owner` check in `workspace_guards.enforce` — `principal.user_id == workspace.created_by` with role `user` is upgraded to `operator` for the duration of the request

---

## 2.0 Invited Member Default Role

**Status:** DONE

Invited team members must default to `user` (read-only) until the workspace owner explicitly grants them mutation access. This preserves least-privilege for non-creator members and keeps `admin` fully manual (admin controls billing lifecycle events with financial impact).

**Dimensions:**
- 2.1 DONE Workspace invite flow sets `publicMetadata.role = "user"` (or omits the claim, which resolves to `user` by default)
- 2.2 DONE Workspace owner can promote an invited member to `operator` via API; promotion is scoped to the owning workspace only
- 2.3 DONE Role escalation events (user → operator) are written to `workspace_billing_audit`
- 2.4 DONE `admin` role is never auto-assigned; it remains a manual Clerk dashboard operation or explicit API call by an existing admin

---

## 3.0 Verification Units

**Status:** DONE

### 3.1 Unit Tests

**Dimensions:**
- 3.1.1 DONE Workspace creator with no role claim receives `operator` access (Option B path)
- 3.1.2 DONE Invited user with no role claim stays as `user` and cannot mutate
- 3.1.3 DONE User already holding `operator` or `admin` is not downgraded by workspace creation handler
- 3.1.4 DONE Role escalation audit row is written on user → operator promotion

### 3.2 Integration Tests

**Dimensions:**
- 3.2.1 DONE New signup → create workspace → `zombiectl workspace sync` returns 200 without manual Clerk intervention
- 3.2.2 DONE Invited team member `zombiectl workspace sync` returns 403 until promoted to `operator`
- 3.2.3 DONE `zombiectl whoami` shows correct role after workspace creation
- 3.2.4 DONE 403 error response body includes role context: `"Your role is 'user'. Contact your workspace owner to request operator access."`

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 New signup → create workspace → `zombiectl workspace sync` succeeds without manual Clerk dashboard intervention
- [x] 4.2 Invited team member cannot mutate until workspace owner promotes them to `operator`
- [x] 4.3 Existing API key auth continues to work as `admin`
- [x] 4.4 `zombiectl whoami` displays current role
- [x] 4.5 403 error responses include role context in the error message
- [x] 4.6 Workspace creator auto-promotion is scoped to workspaces they created — does not grant `operator` on other workspaces
- [x] 4.7 Role cannot be self-escalated to `admin` via any code path

---

## 5.0 Out of Scope

- `admin` auto-assignment (admin grants billing lifecycle access; remains a manual operation)
- Role visibility UI badge in app settings (deferred to a UX milestone)
- Multi-workspace role federation (each workspace is independent)
- Role revocation or demotion flows (operator → user)
- Self-serve `admin` promotion path
