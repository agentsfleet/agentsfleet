# M5_002: Self-Serve Role Assignment

**Version:** v2
**Milestone:** M5
**Workstream:** 002
**Date:** Mar 24, 2026
**Status:** PENDING
**Priority:** P1
**Depends on:** M5_001 (free-plan exhaustion — RBAC layer landed)
**Batch:** B6 — follow-on after M5_001 merge

---

## Problem

A user who signs up, authenticates via Clerk, and creates a workspace receives the default `user` role. This role is read-only — every mutation handler requires `operator` or `admin`. The user cannot run agents, sync specs, manage harness, or do anything useful until a Clerk org admin manually sets `metadata.role = "operator"` in the Clerk dashboard.

This creates a dead-end onboarding: self-serve signups authenticate successfully but hit 403 on every meaningful action. The product appears broken.

## Decision

Auto-assign `operator` to workspace creators. Reserve `user` for invited team members who haven't been explicitly granted mutation access. `admin` remains manually assigned for billing lifecycle operations.

---

## 1.0 Workspace Creator Gets Operator

**Status:** PENDING

When a user creates a workspace (or is the first user in a tenant), the backend should ensure they have at least `operator` access to that workspace. Two implementation options:

### Option A: Clerk Webhook (Preferred)

On workspace creation, call the Clerk API to set `metadata.role = "operator"` on the user. The next JWT they receive carries the role. No backend auth changes needed.

**Dimensions:**
- 1.1 PENDING Add Clerk SDK call in workspace creation handler to set user metadata
- 1.2 PENDING Handle Clerk API failure gracefully (workspace still created, role assignment retried)
- 1.3 PENDING Idempotent: if user already has `operator` or `admin`, don't downgrade

**Pros:** Single source of truth stays in Clerk. No backend role logic changes.
**Cons:** Requires Clerk API key with user metadata write permission. Small latency on first request after signup (JWT must be re-issued to reflect new role).

### Option B: Backend Role Override

If the JWT has no role claim AND the authenticated user is the workspace owner (creator), the backend treats them as `operator` regardless of token claims.

**Dimensions:**
- 1.1 PENDING Add `workspace_owner` check in `workspace_guards.enforce` — if `principal.user_id == workspace.created_by` and `principal.role == .user`, upgrade to `.operator` for this request
- 1.2 PENDING Store `created_by` on workspace row (may already exist)
- 1.3 PENDING Unit test: workspace creator with no role claim gets operator access
- 1.4 PENDING Unit test: invited user with no role claim stays as user

**Pros:** No external API dependency. Immediate — works on the same JWT.
**Cons:** Role is contextual (per-workspace), not portable. Two sources of truth for role resolution.

### Recommendation

**Option A** for production. Option B as a fallback if Clerk API integration is deferred. Both can coexist — B handles the immediate request, A ensures future JWTs carry the correct role.

---

## 2.0 Invited Team Members Default to User

**Status:** PENDING

When a workspace owner invites a team member, the invitee should default to `user` until explicitly granted `operator` by the workspace owner. This preserves the principle of least privilege for non-creator members.

**Dimensions:**
- 2.1 PENDING Workspace invite flow sets `metadata.role = "user"` (or omits role, which defaults to `user`)
- 2.2 PENDING Workspace owner can promote a member to `operator` via API or app UI
- 2.3 PENDING Role changes are audited in `workspace_billing_audit`

---

## 3.0 Role Visibility in CLI and App

**Status:** PENDING

Users should be able to see their current role and understand what they can do.

**Dimensions:**
- 3.1 PENDING `zombiectl whoami` shows current role
- 3.2 PENDING App UI shows role badge in workspace settings
- 3.3 PENDING 403 error responses include actionable message: "Your role is 'user'. Contact your workspace owner to request operator access."

---

## 4.0 Admin Remains Manual

**Status:** PENDING

`admin` is not auto-assigned. It grants access to billing lifecycle events (`PAYMENT_FAILED`, `DOWNGRADE_TO_FREE`) which have financial impact. Admin assignment stays in Clerk dashboard or via explicit API call by an existing admin.

---

## Acceptance Criteria

- [ ] New signup → create workspace → `zombiectl workspace sync` succeeds without manual Clerk intervention
- [ ] Invited team member cannot mutate until promoted to operator
- [ ] Existing API key auth continues to work as `admin`
- [ ] Role is visible via `zombiectl whoami`
- [ ] 403 errors include role context in error message

## Security Considerations

- Workspace creator auto-promotion must be scoped to workspaces they created, not all workspaces
- Option B must not allow a user to claim ownership of a workspace they didn't create
- Role escalation (user → operator) must be audited
- Role cannot be self-escalated to `admin` via any path
