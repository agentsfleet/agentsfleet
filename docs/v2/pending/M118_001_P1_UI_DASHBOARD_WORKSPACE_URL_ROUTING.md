<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M118_001: Put the workspace in the dashboard URL (Supabase-aligned) — kill implicit active-workspace resolution

**Prototype:** v2.0.0
**Milestone:** M118
**Workstream:** 001
**Date:** Jul 06, 2026
**Status:** PENDING
**Priority:** P1 — the "Couldn't load connectors" class of bug: the dashboard guesses the active workspace from an unvalidated cookie/claim; making it explicit in the URL removes the guess.
**Categories:** UI
**Batch:** B1 — coordinated single-PR route move (nav + layout + pages must move together or nav breaks).
**Depends on:** none. Supersedes the M117 connectors stopgap (reverted, not shipped).
**Provenance:** agent-generated (pre-spec, this session's connectors root-cause investigation + the vendored Supabase Studio reference `oss/supabase/apps/studio`)
**Canonical architecture:** `docs/AUTH.md` §scopes (tenant is the security boundary; `ownsWithinTenant` gates every call). Reference: `oss/supabase/apps/studio` — `/project/[ref]`, `useSelectedProject = useParams().ref`.

---

## Overview

**Goal (testable):** Every workspace-scoped dashboard page reads its workspace id from the route (`/w/[workspaceId]/…`), not from a cookie/claim; `withWorkspaceScope` and the whole cookie/claim resolution+retry machinery is deleted; an un-owned or invalid workspace id in the URL renders `notFound()`; and backend authorization is unchanged (`ownsWithinTenant`, server-side, per call).

**Problem:** The dashboard invents an implicit "active workspace" resolved by `resolveActiveWorkspaceId` (cookie → a dead `workspace_id` claim that is never written → tenant list), **trusts it unvalidated**, and fills it into workspace-scoped API calls. A stale cookie → a workspace the tenant doesn't own → 403 → pages that wrapped their reads in `orFallback` silently re-resolve and retry, but the connectors page (bare `.catch`) can't, so it alone shows "Couldn't load connectors." The concept itself is the anti-pattern: our backend API, our CLI/api-key path, and Supabase Studio all put the workspace/project id **in the URL** and authorize it server-side. The dashboard is the only surface that guesses.

**Solution summary:** Move the 12 workspace-scoped pages under an explicit `/w/[workspaceId]/…` segment; each reads `params.workspaceId`. The `(dashboard)` root redirects once to the first owned workspace; the `[workspaceId]` layout validates the id is owned (else `notFound()`). The switcher becomes navigation (`router.push`), not a cookie write. Delete `withWorkspaceScope`, `resolveActiveWorkspaceId`, `resolveFromList`, `readWorkspaceClaim`, `isWorkspaceRejection`, `orFallback`, `ACTIVE_WORKSPACE_COOKIE`, `type ActiveWorkspace` from `lib/workspace.ts` (keep `listTenantWorkspacesCached`). Tenant/platform pages (api-keys, billing, admin/*) stay at root. **No backend change.**

## PR Intent & comprehension handshake

- **PR title (eventual):** refactor(m118): workspace in the URL — delete implicit active-workspace resolution
- **Intent (one sentence):** The dashboard stops guessing the active workspace; it's an explicit, validated URL segment, matching the backend/CLI/Supabase discipline.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `oss/supabase/apps/studio/hooks/misc/useSelectedProject.ts` (`const { ref } = useParams()`) + `.../withAuth.tsx` + `pages/project/[ref]` — the reference model: selection is the route param, authz is a server-side membership check on that ref.
2. `ui/packages/app/lib/workspace.ts` — the machinery being deleted; understand `withWorkspaceScope`/`resolveActiveWorkspaceId` before removing them. `listTenantWorkspacesCached` is the one survivor.
3. `ui/packages/app/app/(dashboard)/layout.tsx` + `components/layout/{Shell,WorkspaceSwitcher}.tsx` + `app/(dashboard)/actions.ts` — the layout, switcher, and the cookie server actions that change.
4. `src/agentsfleetd/http/handlers/common_authz.zig` (`authorizeWorkspace` = `ownsWithinTenant`) — the authorization boundary that STAYS. The URL ref is a UX hint; this is the gate.
5. This session's connectors investigation (the spec's provenance) — why the implicit resolution is the root cause.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/layout.tsx` | CREATE | Workspace layout: read+validate `params.workspaceId` ∈ owned list, else `notFound()`; render Shell |
| `ui/packages/app/app/(dashboard)/{page,approvals,approvals/[gateId],events,fleets,fleets/[id],fleets/new,integrations,secrets,settings/models,settings/defaults,settings/security}` | MOVE→`w/[workspaceId]/…` | The 12 workspace-scoped pages read `params.workspaceId` instead of `withWorkspaceScope`/`resolveActiveWorkspaceId` |
| `ui/packages/app/app/(dashboard)/layout.tsx` | EDIT | Root layout: resolve default owned workspace once → `redirect('/w/<id>')`; keep the switcher's workspace-list fetch |
| `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` | EDIT | `pick(id)` → `router.push('/w/<id>/<current-subpath>')`; drop `onSwitch` cookie action |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | Derive active id from the route param; prefix all nav hrefs with `/w/<id>` for workspace items; leave tenant/platform items at root |
| `ui/packages/app/components/layout/CreateWorkspaceDialog.tsx` | EDIT | On create → `router.push('/w/<newId>')` instead of cookie write + refresh |
| `ui/packages/app/app/(dashboard)/actions.ts` | EDIT | Delete `setActiveWorkspace`/`writeActiveWorkspace` cookie actions; `createWorkspaceAction` returns the new id (no cookie) |
| `ui/packages/app/app/(dashboard)/admin/models/actions.ts` | EDIT | Replace its `withWorkspaceScope` call site with the explicit workspace id it already has in scope |
| `ui/packages/app/lib/workspace.ts` | EDIT | Delete the resolution/retry machinery; keep only `listTenantWorkspacesCached` |
| `ui/packages/app/tests/**` (~11 files) | EDIT | Update page/layout/action/workspace tests to the URL model |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC/ORP (delete the dead resolution machinery + orphan-sweep every removed export), NLR (touch-it-fix-it on each moved page), NLG (no "legacy resolver" framing left behind).
- **`dispatch/write_ts_adhere_bun.md`** — every moved/edited `*.tsx`: `const` discipline, `import type`, path aliases, no `any`, UI-GATE (design-system primitives — Shell/Switcher already use them), no new arbitrary tokens. `no-console` stays clean.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI Substitution / DESIGN TOKEN | yes | pages/Shell/Switcher already compose design-system primitives; the move adds no raw HTML or arbitrary utilities |
| File & Function Length (≤350/≤50/≤70) | yes | pages move largely intact; watch `Shell.tsx`/`layout.tsx` growth as href-prefixing is added — split a nav helper if it crosses 350 |
| UFS | yes | the `/w` route prefix + segment name become named constants (`WORKSPACE_ROUTE_PREFIX`); no duplicated path literals across pages/nav |
| ZIG / SCHEMA / LOGGING / ERROR REGISTRY | no | UI-only; no backend, no schema, no Zig |

## Prior-Art / Reference Implementations

- **Reference (the whole design):** `oss/supabase/apps/studio` — `useSelectedProject = useParams().ref` (`hooks/misc/useSelectedProject.ts:7`), routes `pages/project/[ref]` + `pages/org/[slug]`, `withAuth.tsx` gates on `router.query.ref`, `useCheckPermissions` keys on the URL ref. Selection is the route; authz is a server-side membership check. Mirror this shape.
- **Reference (our own correct discipline):** the api-key path — `/v1/workspaces/{ws}/…` with caller-supplied ws + `ownsWithinTenant`. The dashboard converges onto this.
- **Divergence:** we use the workspace **UUID** as the URL segment (stable, matches `/v1/workspaces/{ws}` exactly, rename-proof), not the display name — see §2 default.

## Sections (implementation slices)

### §1 — Move the 12 workspace-scoped pages under `/w/[workspaceId]/`

Relocates every page that fetches workspace data so its workspace comes from `params.workspaceId`. **Implementation default:** route prefix `/w/[workspaceId]` (short, like Supabase's `/project/[ref]`); each page signature becomes `({ params }: { params: { workspaceId: string } })` and passes `params.workspaceId` straight to the existing API clients. `fleets/[id]` and `approvals/[gateId]` keep the resource id as a second param.

- **Dimension 1.1** — the 10 data-scoped pages read `params.workspaceId` and call the API directly; no `withWorkspaceScope`/`resolveActiveWorkspaceId` import remains in any of them → Test `test_pages_read_workspace_param`
- **Dimension 1.2** — the 2 concept-only pages (`settings/defaults`, `settings/security`) live under the segment too → Test `test_concept_pages_under_segment`

### §2 — Entry redirect + ownership guard

Resolves the default workspace once at entry and validates the URL id. **Implementation default:** the `(dashboard)` root layout resolves the first owned workspace from `listTenantWorkspacesCached` and `redirect('/w/<id>')`; no workspaces → the "create workspace" empty state. The `w/[workspaceId]/layout.tsx` checks `params.workspaceId ∈ owned list` → else `notFound()`. This is a UX guard; the security gate remains `ownsWithinTenant` server-side. **URL id = workspace UUID** (alternative: name-slug — rejected, breaks on rename + cross-tenant collision).

- **Dimension 2.1** — visiting `/(dashboard)` (no ws) redirects to `/w/<first-owned-id>/` → Test `test_root_redirects_to_default_workspace`
- **Dimension 2.2** — an un-owned/invalid `workspaceId` in the URL renders `notFound()`, never another workspace's data → Test `test_unowned_workspace_notfound`
- **Dimension 2.3** — a tenant with zero workspaces sees the create-workspace empty state, not a crash → Test `test_no_workspace_empty_state`

### §3 — Switcher & create-flow become navigation, not cookie writes

Turns workspace selection into a URL change. **Implementation default:** `WorkspaceSwitcher.pick(id)` → `router.push` preserving the current sub-path (`/w/<id>/<subpath>`); `CreateWorkspaceDialog` → `router.push('/w/<newId>')` on success. The `active_workspace_id` cookie and its server actions are deleted.

- **Dimension 3.1** — selecting a workspace navigates to `/w/<id>/…` (same sub-path) and writes no cookie → Test `test_switcher_navigates_no_cookie`
- **Dimension 3.2** — creating a workspace routes to the new workspace's URL → Test `test_create_routes_to_new_workspace`

### §4 — Delete the resolution machinery (Dead Code Sweep)

Removes the implicit-resolution code now that the URL is authoritative. **Implementation default:** delete `withWorkspaceScope`, `resolveActiveWorkspaceId`, `resolveFromList`, `readWorkspaceClaim`, `isWorkspaceRejection`, `orFallback`, `ACTIVE_WORKSPACE_COOKIE`, `type ActiveWorkspace` from `lib/workspace.ts`; keep `listTenantWorkspacesCached`. Delete the cookie server actions from `actions.ts`.

- **Dimension 4.1** — the deleted exports have zero remaining importers in `app/**` → Test `test_resolution_machinery_removed`
- **Dimension 4.2** — `lib/workspace.ts` exports only `listTenantWorkspacesCached` (+ its types) → Test `test_workspace_module_slimmed`

## Interfaces

```
URL scheme (dashboard):
  /w/[workspaceId]/                     ← home            (workspace-scoped)
  /w/[workspaceId]/{fleets,fleets/new,fleets/[id],integrations,secrets,events,
                    approvals,approvals/[gateId],settings/models,
                    settings/defaults,settings/security}
  /settings/{api-keys,billing}          ← TENANT-scoped   (stay at root)
  /admin/{models,runners}               ← PLATFORM-scoped (stay at root)
  /(dashboard)  →  redirect  →  /w/<first-owned-id>/

Backend UNCHANGED: /v1/workspaces/{ws}/… with server-side ownsWithinTenant.
The URL workspaceId is a UX selector; it is NEVER an authorization input.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Un-owned workspaceId in URL | hand-edited/shared link to another tenant's ws | `[workspaceId]` layout → `notFound()`; backend also 403s any data call (defence in depth) |
| Deleted/stale workspaceId | bookmark to a since-deleted ws | not in owned list → `notFound()`; root entry still redirects to a valid default |
| Zero workspaces | brand-new tenant mid-provision | create-workspace empty state (§2.3), never a broken page |
| Someone trusts the URL id for authz | a future handler skips `ownsWithinTenant` | Invariant 1 + a negative test; the URL is a hint, authz is server-side |
| Nav href not prefixed | a workspace nav item left root-relative | breaks navigation within a workspace → covered by a nav-href test |

## Invariants

1. The URL `workspaceId` is a UX selector only — **authorization is always `ownsWithinTenant`, server-side, per call.** No dashboard code authorizes *from* the URL id. (Enforced by the un-owned→`notFound` guard + backend gate; a negative test asserts an un-owned id never renders data.)
2. No cookie/claim workspace resolution remains — enforced by grep (Dimension 4.1) that the deleted exports have no importers.
3. Tenant/platform pages carry no workspace segment — enforced by the route map + nav-href tests.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `workspace_switched` (existing) | product | user picks a workspace | workspace id, source | no PII | `test_switcher_navigates_no_cookie` |

The existing `workspace_switched` PostHog event moves from the switcher's cookie-action path to the `router.push` path — same event, same properties; no new analytics, no funnel change. Analytics group binding (`setAnalyticsContext`) now derives from the route param.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_pages_read_workspace_param` | each moved page renders with `params.workspaceId="ws_1"` → calls its API client with `ws_1`; no workspace-resolution import present |
| 1.2 | unit | `test_concept_pages_under_segment` | `settings/defaults` + `settings/security` resolve under `/w/[workspaceId]/` |
| 2.1 | integration | `test_root_redirects_to_default_workspace` | GET `(dashboard)` root with 2 owned ws → `redirect('/w/<first>')` |
| 2.2 | integration | `test_unowned_workspace_notfound` | `params.workspaceId` not in owned list → `notFound()`; no data client called |
| 2.3 | unit | `test_no_workspace_empty_state` | owned list empty → create-workspace empty state, no throw |
| 3.1 | unit | `test_switcher_navigates_no_cookie` | `pick("ws_2")` → `router.push('/w/ws_2/…')`; no `cookies().set` |
| 3.2 | unit | `test_create_routes_to_new_workspace` | create success → `router.push('/w/<newId>')` |
| 4.1 | unit | `test_resolution_machinery_removed` | grep `app/**` → 0 importers of `withWorkspaceScope`/`resolveActiveWorkspaceId`/`orFallback`/`ACTIVE_WORKSPACE_COOKIE` |
| 4.2 | unit | `test_workspace_module_slimmed` | `lib/workspace.ts` exports === `{ listTenantWorkspacesCached }` (+ types) |
| — | regression | `test_tenant_pages_stay_at_root` | api-keys/billing/admin pages resolve at root, unchanged |
| — | e2e | `test_e2e_workspace_url_flow` | drive: land → redirected to `/w/<id>`; switch → URL changes + data reloads; deep-link an owned ws → loads; connectors loads (the original bug) |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Workspace pages read the URL param (§1) | `grep -rL 'params' ui/packages/app/app/\(dashboard\)/w/\[workspaceId\]/**/page.tsx` | no page missing `params` | P1 | |
| R2 | Resolution machinery gone (§4) | `grep -rn 'withWorkspaceScope\|resolveActiveWorkspaceId\|ACTIVE_WORKSPACE_COOKIE\|orFallback' ui/packages/app/app ui/packages/app/components` | no output | P0 | |
| R3 | workspace.ts slimmed (§4) | `grep -c 'export' ui/packages/app/lib/workspace.ts` | 1 export (+ its types) | P1 | |
| R4 | Un-owned id → notFound (Invariant 1) | `make test-unit-agentsfleet` (runs `test_unowned_workspace_notfound`) | exit 0 | P0 | |
| R5 | Tenant/platform pages still at root | `test -f ui/packages/app/app/\(dashboard\)/settings/api-keys/page.tsx` | exit 0 | P1 | |
| R6 | Diff inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the table | P0 | |
| S1 | Unit + component tests pass | ui package `vitest run` | exit 0 | P0 | |
| S2 | Lint + typecheck clean | `bun run lint` + `tsc --noEmit` | exit 0 | P0 | |
| S3 | e2e workspace flow (incl. connectors) | `{test-e2e command}` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | Orphan sweep (deleted exports) | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — pages are MOVED (git mv), not deleted; the old root paths must not remain | `test ! -f ui/packages/app/app/\(dashboard\)/integrations/page.tsx` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `withWorkspaceScope` | `grep -rn 'withWorkspaceScope' ui/packages/app/app ui/packages/app/components` | 0 matches |
| `resolveActiveWorkspaceId` | `grep -rn 'resolveActiveWorkspaceId' ui/packages/app` | 0 (outside its own deletion) |
| `orFallback` / `isWorkspaceRejection` | `grep -rn 'orFallback\|isWorkspaceRejection' ui/packages/app/app` | 0 matches |
| `ACTIVE_WORKSPACE_COOKIE` | `grep -rn 'ACTIVE_WORKSPACE_COOKIE\|active_workspace_id' ui/packages/app` | 0 matches |

## Out of Scope

- **Backend changes** — none. `/v1/workspaces/{ws}/…` + `ownsWithinTenant` already do the right thing.
- **Writing `workspace_id` into the Clerk JWT** — the URL is authoritative; a stale-able claim adds nothing. Explicitly rejected.
- **Name-slug URLs** (`/w/cosy-shore-605`) — considered; rejected for rename-stability + cross-tenant collision. UUID segment ships.
- **Cross-device "last workspace" memory** — the root entry redirects to first-owned; a remembered-last is a follow-up (would be a small client-side store, never an authz input).
- **Splitting Settings** — workspace-scoped settings (models/defaults/security) move under the segment; tenant settings (api-keys/billing) stay at root. Nav handles the two roots; no unification in scope.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user opens Integrations, it loads its connectors; the URL reads `/w/019f…/integrations`; they can bookmark or share it and it lands on the same workspace; switching workspaces visibly changes the URL and the data.
2. **Preserved user behaviour** — Every page shows the same data; the switcher still switches; tenant settings/billing/admin unchanged. Only *how the workspace is selected* changes (URL, not cookie).
3. **Optimal-way check** — Yes: URL-explicit selection is the reference design (Supabase) and matches our own backend/CLI; it deletes the bug class rather than hardening a workaround.
4. **Rebuild-vs-iterate** — A deliberate refactor (route restructure), chosen over the M117 patch/validate-hint stopgap because the implicit-active-workspace concept is itself the defect. Determinism improves (no per-render guess).
5. **What we build** — a `/w/[workspaceId]` segment + layout guard, param-reading pages, a navigation-based switcher, a slimmed `workspace.ts`.
6. **What we do NOT build** — JWT workspace claim, name-slugs, cross-device memory, backend changes (all Out of Scope).
7. **Fit with existing features** — Compounds with the workspace switcher + the tenant/workspace model; must not weaken `ownsWithinTenant` (Invariant 1).
8. **Surface order** — UI-only; the backend/CLI already carry the workspace in the path, so this makes the dashboard consistent with them.
9. **Dashboard restraint** — the URL now honestly names what you're looking at; no control implies access you lack (an un-owned id 404s).
10. **Confused-user next step** — a stale bookmark 404s to a clean "not found" and the root redirect lands them on a valid workspace — self-serve, no ticket.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One workstream, one coordinated PR — the layout, nav, and all pages must move together (a partial move breaks navigation). Sections split the move / guard / switcher / deletion for reviewability.
- **Alternatives considered:** (a) M117 `orFallback` stopgap — rejected: hardens the anti-pattern. (b) B-full validate-the-hint — rejected: keeps implicit resolution. (c) name-slug URLs — rejected (rename/collision). This is the Supabase-aligned end state.
- **Patch-vs-refactor verdict:** a **refactor** — the implicit-active-workspace concept is the defect; a patch would only make the guess safer. Large-ish blast radius (12 page moves + nav + lib deletion), but UI-only and mechanical; the security boundary (`ownsWithinTenant`) is untouched.

## Discovery (consult log)

- **Consults** — Indy chose the Supabase-aligned URL model over the M117 stopgap and the validate-hint (B-full) option this session, after reviewing the vendored `oss/supabase/apps/studio` reference (`useSelectedProject = useParams().ref`). Security-boundary reasoning captured: tenant is the boundary, workspace is a namespace, the URL id is a hint and authz stays `ownsWithinTenant`.
- **Metrics review** — no new events; `workspace_switched` moves paths only.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: {empty at creation}
- **Deferrals** — {empty at creation}
