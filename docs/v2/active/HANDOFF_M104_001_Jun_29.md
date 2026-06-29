# HANDOFF — M104_001 Scope-Based Authorization

> Ephemeral pickup brief. Delete at CHORE(close). Source of truth is the spec:
> `docs/v2/active/M104_001_P1_API_DOCS_INFRA_SCOPE_BASED_AUTHZ.md`. This file only
> orients the next agent; it never overrides the spec.

## Scope / Status

Replacing role-based authorization (`AuthRole` ladder + `platform_admin` bool) with
explicit `resource:action` scopes across the agentsfleetd Zig backend. **CHORE(open)
is done; no implementation code written yet.**

- ✅ Spec authored, audited (SPEC TEMPLATE GATE clean), grounded in Sentry/Supabase/bun.
- ✅ CHORE(open): spec moved `pending/` → `active/`, `Status: IN_PROGRESS`, `Branch:` set,
  `Test Baseline: unit=2214 integration=213` recorded, worktree created.
- ⏳ Implementation — NOT started. Begin at §1 of the spec.
- This milestone **blocks M103_001** (template catalog), which gates on `template:write`.

## Working Tree

- Worktree: `/Users/kishore/Projects/agentsfleet-m104-scope-authz` (work HERE; do not edit the main checkout).
- `git status`: clean.
- Branch `feat/m104-scope-authz`, **local only — not pushed, no PR yet.**
- Contains all main-branch design commits (verify `git log --oneline -5`):
  - `736d82be` CHORE(open) activate M104
  - `cc1de3d5` ground scope authz in Sentry/Supabase/bun
  - `a931b64b` add M104_001 + M103 repoint
  - `02662199` add M103_001 + retire M96_001
- `bun install` already run (888 packages). For CLI-touching tests also run
  `cd cli && bun install && bun run build` (Zig-only work does not need it).

## Branch / PR

- Branch: `feat/m104-scope-authz` · PR: none yet (open after VERIFY + CHORE(close)).
- CI: n/a until pushed. Do NOT force-push once a PR exists.

## Running Processes

None persistent. No tmux sessions, dev servers, or watchers left running.

## Tests / Checks

- Test Baseline recorded: **unit=2214 integration=213** (`make _lint_zig_test_depth`).
  VERIFY's Test Delta compares against this — a code-adding diff with zero unit delta
  must be justified or sent back to EXECUTE.
- Nothing built/tested yet for this milestone. Eval set (spec §Eval Commands):
  `make test-unit-agentsfleetd && make test-integration && make lint && make test`,
  both Linux cross-compiles, `gitleaks detect`, and the legacy sweep
  `rg -n "RequireRole|platformAdmin|platform_admin|AuthRole|\.atLeast\(|\.allows\(" src | grep -v test`.

## Next Steps (ordered — follow the spec's Sections)

1. **PLAN + handshake** — restate the intent, list `ASSUMPTIONS I'M MAKING`. Read the spec's
   "Implementing agent — read these first" (Sentry/Supabase/bun citations + local auth files).
2. **§1 — scope catalog** (`src/agentsfleetd/auth/scopes.zig`): the `Scope` enum, the
   `read<write<admin` hierarchy as a comptime map (NOT string-inferred), provisioning bundles,
   `satisfiesAny`. Add `principal.scopes`; surface the already-parsed claim in `claims.zig`.
3. **§2 — declarative gate**: `route_scopes.zig` (comptime route→required-scope table, bun pattern)
   + `require_scope.zig` (one central any-of gate). Keep `authorizeWorkspace` ownership untouched.
4. **§3 — migrate all 65+ gates** (the enumeration in the spec's Discovery is the lossless
   checklist) + the cross-tenant override (`workspace:read:any`/`workspace:write:any`) with
   `cross_tenant_audit.zig` emitting an audit record on every bypass.
5. **§4 — delete** `rbac.zig`, `require_role.zig`, `platform_admin.zig`; remove `AuthRole`/`platform_admin`.
6. **§5 — docs + Clerk**: rewrite `docs/AUTH.md` to the scope catalog; document the session-token
   `scopes` claim + bundle provisioning in `playbooks/founding/03_priming_infra/001_playbook.md`.
7. VERIFY (`/write-unit-test` first), `/review`, then CHORE(close) → PR.

## Risks / Gotchas

- **Security boundary, every authenticated route.** Big-bang cutover (Indy's call: pre-2.0, not in
  production) — there is no staged dual-run, so a missed gate = an unguarded route. The 65-gate
  enumeration is the checklist; the legacy-sweep grep must come back production-empty.
- **Two axes stay separate.** Scopes = capability; `authorizeWorkspace`/tenant-isolation = ownership.
  Do NOT collapse them — deleting the ownership check is a cross-tenant IDOR. Only the audited
  `workspace:{read,write}:any` override bypasses the tenant-id match.
- **Bundles are provisioning-only.** `workspace_admin`/`workspace_member` expand to scopes at
  provisioning; NO route may gate on a bundle name (gates take `Scope` enum values only).
- **Sentry scope list was read off `master`** — borrow the *shape* (hierarchy-as-data, any-of,
  roles-as-bundles), not their literal scopes; our catalog is derived from our own 65-gate sweep.
- **`docs/AUTH.md` still describes the OLD role model** until §5 lands — don't trust it mid-flight.
- Tenant isolation today is app-enforced (the `AND tenant_id` clause in `authorizeWorkspace`);
  there is **no Postgres Row-Level Security policy backstop yet** — keep the clause intact.
- M103_001 sits in `pending/` depending on this; don't start it until M104 merges.

## Pickup command

`/pickup` in `/Users/kishore/Projects/agentsfleet-m104-scope-authz`, then read the spec and start at Next Step 1.
