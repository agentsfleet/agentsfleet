<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh.
-->

# M101_001: Dashboard perf — unblock workspace-scoped fetches, stream shells, split heavy islands

**Prototype:** v0.x
**Milestone:** M101
**Workstream:** 001
**Date:** Jun 24, 2026
**Status:** IN_PROGRESS — parked on two environment-gated checks. §1 (resolver), §2 (fallback + 11-route rewire), §3 (Suspense streaming on `/fleets`, `/events`, `/approvals`), §4 (billing dedup), §5 (code-split: 5 `next/dynamic` island shims) DONE + unit/RTL-tested (1044 passed), build clean. **Remaining (need a live authenticated stack, unavailable in this frontend worktree):** the e2e workspace-fetch-audit run and the §5.3 assistant-ui live QA — both documented in Verification Evidence with exact run commands; run in CI / a provisioned dev env before merge.
**Priority:** P1 — the dashboard's hottest path pays two serial round-trips per navigation and ships interaction-only islands in the initial bundle.
**Categories:** UI
**Batch:** B1 — frontend-only; the M101_002 backend endpoints are an independent sibling PR.
**Branch:** feat/m101-frontend-perf
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, performance investigation of `ui/packages/app`).

> **Provenance is load-bearing.** Agent-generated — cross-check every claim against the codebase before EXECUTE.

**Canonical architecture:** no dedicated data-layer architecture doc; the shape is defined by `ui/packages/app/lib/workspace.ts` and the App-Router server-component convention. Backend authorization boundary is documented inline at `src/agentsfleetd/http/handlers/fleets/list.zig` + `src/agentsfleetd/http/workspace_guards.zig`.

---

## Implementing agent — read these first

1. `ui/packages/app/lib/workspace.ts` — current `resolveActiveWorkspace`; always lists workspaces before picking. The function being split.
2. `ui/packages/app/app/(dashboard)/fleets/page.tsx` + `app/(dashboard)/page.tsx` — the serial resolve→data pattern, and the home page's `<Suspense>`+`Skeleton`+async-child streaming shape to mirror everywhere.
3. `ui/packages/app/components/domain/FleetThreadDynamic.tsx` — the established `next/dynamic` (React.lazy + `ssr:false`) code-split shim; §5 extends this exact pattern to other islands.
4. `src/agentsfleetd/http/handlers/fleets/list.zig` (~line 41) + `src/agentsfleetd/http/workspace_guards.zig` — proof `authorizeWorkspace` is the real authz boundary (returns `ERR_FORBIDDEN` for a non-owned workspace). The safety basis for trusting the cookie hint.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Unblock dashboard data fetches from workspace lookup; stream shells; split islands
- **Intent (one sentence):** Workspace-scoped pages start their data fetch from a trusted cookie/JWT hint (not a list round-trip), paint their shell before data resolves, and load interaction-only client islands on demand.
- **Handshake (agent fills at PLAN, before EXECUTE):**
  - Restated intent: remove the workspace-list call from the data critical path; stream the page chrome; keep heavy client chunks out of the initial bundle.
  - `ASSUMPTIONS I'M MAKING:`
    1. Backend authorizes every workspace-scoped route by token tenant, so an invalid hint yields `ERR_FORBIDDEN`, never cross-tenant data. **Verified.**
    2. Routes consume only `workspace.id`.
    3. The project uses `next/dynamic` for SSR-opt-out splitting (Next 16 forbids `ssr:false` in Server Components); raw `React.lazy` is reserved for client-only subtrees already under a Suspense boundary.
  - `-> Correct me now or I'll proceed with these.`

---

## Product Clarity

1. **Successful user moment** — operator clicks a sidebar item; the header paints instantly, data resolves in one round-trip, and opening a dialog/install flow doesn't stutter because its code streams in on first use rather than at page load.
2. **Preserved user behaviour** — switcher lists every workspace; switching rewrites the cookie and revalidates; single-workspace new signup lands on populated pages; a stale active workspace degrades to a valid one, not an error; every dialog/flow still opens and works.
3. **Optimal-way check** — optimal is the active id in the session token with zero round-trips; the claim exists but isn't guaranteed current, so cookie-hint + backend-rejection fallback is the acceptable gap.
4. **Rebuild-vs-iterate** — iterate; the data layer is sound and a rebuild only adds determinism risk.
5. **What we build** — a cheap id resolver, a 403-fallback wrapper, rewired routes, per-route Suspense shells, `cache()` billing dedup, and `next/dynamic` shims for interaction-only islands.
6. **What we do NOT build** — new backend endpoints (M101_002), `useOptimistic` rework of every button (follow-up), session-token workspace persistence (AUTH spec).
7. **Fit with existing features** — compounds with the switcher, the `cache()` dedup, and the existing `FleetThreadDynamic` split; must not destabilize the switch-workspace action or SSR of above-the-fold content.
8. **Surface order** — UI-only.
9. **Dashboard restraint** — invisible plumbing; no new controls or claims.
10. **Confused-user next step** — a stale cookie self-heals via the 403 fallback; the user sees data, not an error.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline.
- **`dispatch/write_ts_adhere_bun.md`** — diff is `*.ts`/`*.tsx`; file-shape, `const`/import discipline, no arbitrary literals, design-system primitives for fallbacks.
- **`docs/AUTH.md`** — the resolver reads the Clerk session/JWT claim.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | N/A — no `*.zig` |
| PUB / Struct-Shape | no | N/A |
| File & Function Length (≤350/≤50/≤70) | yes — `lib/workspace.ts` grows | keep resolver + wrapper under caps; split to `lib/workspace-resolve.ts` if near 350 |
| UFS (repeated/semantic literals) | yes | reuse `ACTIVE_WORKSPACE_COOKIE`; add named const for the claim key; one shared `Skeleton` height const per island family |
| UI Substitution / DESIGN TOKEN | yes | Suspense + `next/dynamic` fallbacks reuse `@agentsfleet/design-system` `Skeleton`; no arbitrary Tailwind |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | N/A |

---

## Overview

**Goal (testable):** a workspace-scoped render issues at most one `GET /v1/tenants/me/workspaces`, starts its primary fetch without awaiting that list when a cookie/JWT hint exists, paints its `PageHeader` before data resolves, and the per-route initial JS chunk excludes interaction-only island code (dialogs, install flow, provider selector).

**Problem:** (1) every workspace-scoped page resolves the active workspace by listing all workspaces, then fetches data — two serial round-trips, re-paid on every client navigation because `cache()` is render-scoped. (2) most routes `await Promise.all([...])` for everything before returning a byte, so the shell can't stream. (3) interaction-only islands load eagerly in the route's first chunk.

**Solution summary:** split `resolveActiveWorkspace` into a cheap hint resolver (cookie → claim → cached-list fallback) plus a 403-fallback wrapper that re-resolves against the authoritative list once; rewire routes to use it; wrap each route's data region in `<Suspense>` so chrome streams; dedup billing with `cache()`; and wrap click-gated islands in `next/dynamic` shims mirroring `FleetThreadDynamic`.

---

## Prior-Art / Reference Implementations

- **UI streaming** → `app/(dashboard)/page.tsx` (`StatusTiles`/`RecentActivity` under `<Suspense>` with `Skeleton`).
- **Resolver dedup** → `lib/workspace.ts` `listTenantWorkspacesCached = cache(...)`.
- **Code-split** → `components/domain/FleetThreadDynamic.tsx` — `next/dynamic` with `ssr:false` + `Skeleton` loading.
- **Backend boundary** → `src/agentsfleetd/http/handlers/fleets/list.zig` `authorizeWorkspace`.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/workspace.ts` | EDIT | add `resolveActiveWorkspaceId` + `withWorkspaceScope`; constant for claim key |
| `ui/packages/app/lib/workspace.test.ts` | CREATE | unit tests: resolver precedence + fallback |
| `ui/packages/app/lib/api/tenant_billing.ts` | EDIT | export `getTenantBillingCached = cache(getTenantBilling)` |
| `ui/packages/app/app/(dashboard)/fleets/page.tsx` | EDIT | resolver; Suspense shell |
| `ui/packages/app/app/(dashboard)/fleets/[id]/page.tsx` | EDIT | resolver + fallback wrapper |
| `ui/packages/app/app/(dashboard)/fleets/new/page.tsx` | EDIT | resolver |
| `ui/packages/app/app/(dashboard)/events/page.tsx` | EDIT | resolver; Suspense shell |
| `ui/packages/app/app/(dashboard)/approvals/page.tsx` | EDIT | resolver; Suspense shell |
| `ui/packages/app/app/(dashboard)/approvals/[gateId]/page.tsx` | EDIT | resolver |
| `ui/packages/app/app/(dashboard)/credentials/page.tsx` | EDIT | resolver |
| `ui/packages/app/app/(dashboard)/settings/models/page.tsx` | EDIT | resolver |
| `ui/packages/app/app/(dashboard)/page.tsx` | EDIT | resolver; billing via cached reader |
| `ui/packages/app/components/domain/island-dynamic/*.tsx` | CREATE | `next/dynamic` shims for click-gated islands (dialogs, install flow, provider selector) |
| call sites of the wrapped islands | EDIT | import the dynamic shim instead of the eager component |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five sections — resolver, route rewire+fallback, streaming shells, billing dedup, island splitting. The resolver is the keystone; streaming and splitting are independent wins layered on the same route edits.
- **Alternatives considered:** (a) **persist active workspace in the session JWT** — removes the hint/fallback entirely but is an AUTH-boundary change with its own review profile; deferred. (b) **per-route `Promise.all` parallelize only** — still lists on every nav and scatters logic; rejected. (c) **raw `React.lazy` everywhere** — rejected for SSR-opt-out islands because Next 16 forbids `ssr:false` outside a client shim; `next/dynamic` is the project convention.
- **Patch-vs-refactor verdict:** **refactor** (contained: one module + call-site rewire + shims) because the data-path win requires changing *what the fetch depends on*. The JWT-claim refactor is the long game — named as a follow-up AUTH spec, not mud-patched.

---

## Sections (implementation slices)

### §1 — Cheap active-workspace-id resolver

`resolveActiveWorkspaceId(token)` returns the id from the `active_workspace_id` cookie, else the `metadata.workspace_id` JWT claim, else the first cached-list item, else `null`. Only the final fallback hits the backend. **Implementation default:** return `{ id, source: "cookie" | "claim" | "list" } | null` so callers know if the id is hint-derived (un-validated) or authoritative.

- **Dimension 1.1** — cookie present → that id, `source:"cookie"`, no list fetch → `test_resolver_prefers_cookie_no_fetch`
- **Dimension 1.2** — no cookie, claim present → claim id, no list fetch → `test_resolver_falls_to_claim`
- **Dimension 1.3** — no hint → cached list, first id, `source:"list"` → `test_resolver_falls_to_list`
- **Dimension 1.4** — no hint, empty list → `null` → `test_resolver_null_when_no_workspace`

### §2 — 403-fallback wrapper + route rewire

`withWorkspaceScope(token, fn)` resolves the id, runs `fn(id)`, and on an `ApiError` forbidden/not-found from a hint-derived id, re-resolves against the authoritative list and retries `fn` exactly once with the list-derived id (when it differs). List-derived ids never retry. The cookie is **not** mutated here — Next 16 Server Components cannot write cookies; a stale cookie self-heals on the next workspace switch (which runs in a Server Action). Routes replace `resolveActiveWorkspace` with the resolver and wrap their primary fetch.

- **Dimension 2.1** — hint id 403 then success → re-resolves via list, fn called twice, returns 2nd result → `test_fallback_reresolves_on_forbidden`
- **Dimension 2.2** — list-derived id 403 → fn once, ApiError propagates → `test_fallback_no_retry_on_authoritative_id`
- **Dimension 2.3** — no route awaits the list for its data call when a cookie is set → `test_routes_use_resolver` (grep + typecheck)
- **Dimension 2.4** — hint id 403 and the authoritative list is empty (last workspace deleted) → returns `null` (no-workspace state), does NOT throw → `test_fallback_null_when_list_empty_after_reject`

### §3 — Per-route Suspense streaming — DONE

Each rewired route wraps its data region in `<Suspense>` with a `Skeleton` fallback and moves the fetch into an exported async child (`FleetsData`/`EventsData`/`ApprovalsData`), so `PageHeader`/`PageTitle` streams first. Mirrors the home page's `StatusTiles`/`RecentActivity` split. Note: on `/fleets` the `ExhaustionBanner` now streams *inside* the data region (below the header) rather than above it — consistent with the home page placement.

- **Dimension 3.1** — DONE — `/fleets`, `/events`, `/approvals` render the header synchronously with a skeleton while data is pending → `test_route_shell_streams_before_data` (one shell-streams test per route asserting `PageTitle` present + skeleton present + data content absent: `fleets-routes.test.ts`, `dashboard-placeholder.test.ts`, `approvals-pages.test.ts`)

### §4 — Billing read dedup

`getTenantBillingCached = cache(getTenantBilling)` collapses multiple per-render billing reads to one call.

- **Dimension 4.1** — two billing reads, one render → one backend call → `test_billing_deduped_per_render`

### §5 — Code-split interaction-only islands — DONE (5.1/5.2/5.3-static); 5.3 live-QA pending

Wrap **interaction-only** client components in `next/dynamic` shims (`components/domain/island-dynamic/*Dynamic.tsx`) mirroring `FleetThreadDynamic`, keeping their chunks out of each route's initial JS bundle. Shipped shims: `EditCredentialDialogDynamic`, `AddCredentialFormDynamic`, `CreateWorkspaceDialogDynamic`, `CreateApiKeyDialogDynamic`, `AddRunnerDialogDynamic`. **Scope narrowed from the original list** — the install flow and `ProviderSelector` are above-the-fold *primary* page content (not interaction-only), so they are NOT split (see Discovery, Jun 25 2026). **Pattern:** always-mount `ssr:false` shim (loads the chunk after hydration, out of the initial bundle, preserving all open/close behaviour + exit animations — NOT open-gated unmount, which would drop the Radix exit animation). **Loading fallback per island:** `() => null` for parent-controlled dialogs / disclosure-gated forms (invisible when idle, so a Skeleton would wrongly paint a box); a button-sized `Skeleton` for self-triggering dialogs that own their trigger (reserves the footprint to avoid CLS).

- **Dimension 5.1** — DONE — a route that renders an island does not statically import it; the shim pulls it in via `next/dynamic`, so its module is excluded from the route's initial chunk → `test_island_not_in_initial_chunk` (source assertion in `tests/island-dynamic.test.ts` §5.1; the production `next build` confirms the actual chunk split)
- **Dimension 5.2** — DONE — each shim mounts its inner component after the dynamic-load tick → `test_dynamic_island_mounts_on_open` (RTL, `tests/island-dynamic.test.ts` §5.2, one case per shim)
- **Dimension 5.3** — DONE (static) / live-QA pending — the assistant-ui chat surface (`FleetThread`) is on-brand and fluid. Static guard shipped: `test_fleetthread_uses_design_tokens` asserts FleetThread styling references design-system tokens (`bg-surface-deep`/`border-border`/`text-muted-foreground`), not raw `aui-`-prefixed assistant-ui theme (`tests/island-dynamic.test.ts` §5.3). Live QA (reduced-motion-gated motion, no layout shift on stream, smooth autoscroll) via `/design-review` + browse on `/fleets/[id]` needs the running authenticated app — see Verification Evidence + Discovery for status.

---

## Interfaces

```
// lib/workspace.ts
type ActiveWorkspace = { id: string; source: "cookie" | "claim" | "list" };
resolveActiveWorkspaceId(token: string): Promise<ActiveWorkspace | null>;
withWorkspaceScope<T>(token: string, fn: (workspaceId: string) => Promise<T>): Promise<T | null>;
//   null  → tenant has no workspace (caller renders no-workspace empty state)
//   throws ApiError → non-authz failure (caller's existing .catch handles it)

// lib/api/tenant_billing.ts
getTenantBillingCached(token: string): Promise<TenantBilling | null>;

// components/domain/island-dynamic/<Name>Dynamic.tsx
//   default export: client shim around next/dynamic(() => import("../<Name>"), { ssr:false, loading: Skeleton })
```

No HTTP contract changes — only existing endpoints are issued.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale cookie | active workspace deleted / prior tenant | optimistic call → `ERR_FORBIDDEN`; wrapper re-resolves via list, retries once; user sees valid data (cookie persists until next switch — a render cannot mutate cookies) |
| No workspace | brand-new tenant (no hint, empty list) | resolver returns `null`; route renders existing "No workspace yet" empty state |
| Stale hint + zero workspaces | last workspace deleted while its cookie/claim lingers | optimistic call → 403; re-resolve via list → list empty → `withWorkspaceScope` returns `null` (NOT a thrown error); route renders the empty state |
| List fetch fails (fallback) | backend/network blip | resolver `.catch` → `null`; existing empty state, no crash |
| Forbidden loop | both hint and list id rejected | wrapper retries at most once; list-derived rejection surfaces `ApiError` — no infinite loop |
| Claim malformed | JWT `metadata.workspace_id` not a string | resolver ignores, falls through (existing null-guard) |
| Dynamic chunk load fails | network drop fetching the split chunk | `next/dynamic` shows the loading skeleton; Next retries on next interaction; non-fatal to the page shell |

---

## Invariants

1. `withWorkspaceScope` retries the inner fn **at most once** — boolean guard, asserted by `test_fallback_no_retry_on_authoritative_id`.
2. A hint-derived id is never trusted for cross-tenant access — enforced by backend `authorizeWorkspace`; the wrapper depends on it (covered by `workspace_guards` tests).
3. The resolver issues a round-trip only on the `source:"list"` path — code structure, asserted by `test_resolver_prefers_cookie_no_fetch` (fetch spy not-called).
4. A dynamic island shim renders the same props contract as the eager component — TypeScript prop-type identity (the shim imports the component's `Props` type), enforced at compile time.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_resolver_prefers_cookie_no_fetch` | cookie=`ws_a` → `{id:"ws_a",source:"cookie"}`; fetch spy not called |
| 1.2 | unit | `test_resolver_falls_to_claim` | no cookie, claim=`ws_b` → `{id:"ws_b",source:"claim"}`; spy not called |
| 1.3 | unit | `test_resolver_falls_to_list` | no hint, list=`[ws_c]` → `{id:"ws_c",source:"list"}` |
| 1.4 | unit | `test_resolver_null_when_no_workspace` | no hint, list=`[]` → `null` |
| 2.1 | unit | `test_fallback_reresolves_on_forbidden` | hint `ws_x` (not in list), fn throws 403 then ok for list id → fn ×2, returns 2nd |
| 2.2 | unit | `test_fallback_no_retry_on_authoritative_id` | list id, fn throws 403 → fn ×1, ApiError propagates |
| 2.4 | unit | `test_fallback_null_when_list_empty_after_reject` | hint `ws_x`, fn throws 403, list empty → returns `null`, no throw |
| 2.3 | unit | `test_routes_use_resolver` | grep/typecheck: no route uses `resolveActiveWorkspace` for its data call |
| 3.1 | unit (RTL) | `test_route_shell_streams_before_data` | render `/fleets` with never-resolving data → `PageTitle` + `Skeleton` present |
| 4.1 | unit | `test_billing_deduped_per_render` | two `getTenantBillingCached`, one render → fetch spy ×1 |
| 5.1 | unit | `test_island_not_in_initial_chunk` | shim module imports component via `next/dynamic`; route file has no static import of the component |
| 5.2 | unit (RTL) | `test_dynamic_island_mounts_on_open` | trigger open → skeleton, then island content after import resolves |
| acceptance | e2e | `workspace-fetch-audit` soft-nav | navigate with `active_workspace_id` cookie → list-path audit `total` is 0 |

Regression: precedence tests (1.1–1.4) guard the existing cookie>claim>first behaviour; every dialog/flow e2e that exists today must still pass (islands behave identically, just lazily). Idempotency: the fallback retry-once is the only retry; `test_fallback_no_retry_on_authoritative_id` proves no storm.

---

## Acceptance Criteria

- [x] Resolver returns the cookie hint without a list fetch — `test_resolver_prefers_cookie_no_fetch` (fetch spy not called) passes in the full run
- [x] No workspace-scoped route awaits the list before its data call when a cookie is set — routes use `withWorkspaceScope` / `resolveActiveWorkspaceId` (§1/§2, already shipped)
- [ ] Soft navigation issues zero list fetches with a valid cookie — `AGENTSFLEET_E2E_AUDIT=1 bun run test:e2e:acceptance` — **⏳ env-gated** (no local backend / Clerk creds; run in CI/dev before merge). Mechanism unit-tested.
- [x] Click-gated islands are dynamically imported — 5 shims in `components/domain/island-dynamic/` use `next/dynamic` + `ssr:false`; call sites have no static import of the wrapped components (`tests/island-dynamic.test.ts`)
- [x] `bun run lint` clean · `bun run test` passes (1044) · `bun run build` succeeds
- [x] No NEW file over 350 lines added (pre-existing `fleets-routes.test.ts` 508→519L noted; enforced FLL gate is Zig-only)
- [x] `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: Resolver + fallback unit tests
cd ui/packages/app && bun run test lib/workspace.test.ts && echo "PASS" || echo "FAIL"
# E2: Build — cd ui/packages/app && bun run build
# E3: Tests — cd ui/packages/app && bun run test
# E4: Lint — cd ui/packages/app && bun run lint
# E5: (Zig only — N/A)
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: Orphan sweep — grep -rn "resolveActiveWorkspace" ui/packages/app/app | grep -v test | head
```

---

## Dead Code Sweep

**1. Orphaned files — none expected** (resolver added to existing module; shims are new files).

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | — |

**2. Orphaned references.** If `resolveActiveWorkspace` is fully superseded, remove it and zero its references.

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `resolveActiveWorkspace` (if removed) | `grep -rn "resolveActiveWorkspace\b" ui/packages/app | grep -v test` | 0 (or only the documented switcher/layout path) |

---

## Discovery (consult log)

> Empty at creation. Append as work surfaces consults and decisions.

- Backend-boundary consult (pre-spec): `list.zig` + `workspace_guards.zig` confirm `authorizeWorkspace` rejects foreign workspace ids with `ERR_FORBIDDEN` — the safety basis for trusting the cookie hint.
- Scope consult (Indy, Jun 24 2026): collapse the planned 3-workstream M100 into one M101 frontend spec; include React.lazy/Suspense/code-splitting. Backend endpoints stay out per `dispatch/write_spec.md` (new endpoints get their own PR) → M101_002 follow-up.
- §5 scope decision (Indy delegated, Jun 25 2026): splitting narrowed to **interaction-only islands only** — the five dialogs/forms (Edit credential, Add custom-secret form, Create workspace, Create API key, Add runner). The install flow (`InstallFleet`) and `ProviderSelector` were **dropped from §5**: they are above-the-fold *primary* page content, not interaction-only, so `ssr:false`-splitting them would flash skeletons over the main content and SSR-on splitting adds blast radius for marginal gains. Indy asked Orly to pick the option; CTO call = "click-gated dialogs only" (lowest risk, zero behaviour regression, matches `FleetThreadDynamic`).
- §5 pattern decision (Jun 25 2026): **always-mount `ssr:false` shim**, NOT open-gated unmount (`{open && <Dynamic/>}`). Open-gated would defer the chunk to first-open (marginally better bundle) but unmount-on-close drops the Radix exit animation on every dialog — a real UX regression traded for a marginal win. Always-mount keeps the chunk out of the route's *initial* bundle (loads post-hydration) while preserving all open/close behaviour. Loading fallback is `() => null` for idle-invisible islands (parent-controlled dialogs, the disclosure-gated form) and a button-sized `Skeleton` for self-triggering dialogs (CLS reserve).
- Mutation testing (Indy asked, Jun 25 2026): repo has **none** (no Stryker / config / make target). Coverage is line/branch via vitest + the Playwright acceptance suite. Recommendation: scope a narrow Stryker run over `lib/workspace.ts` (the branchy 403/404 retry keystone) as a follow-up — not bundled into this frontend PR.
- Detail-route stale-hint self-heal (Indy, Jun 25 2026): `/review` + greptile flagged that detail routes (`fleets/[id]`, `approvals/[gateId]`) don't self-heal a stale cookie the way list routes do. **Known latent asymmetry, not a live bug** — not reachable today (no workspace-delete exists; the only stale path is cross-login on a shared browser, which self-heals on the list routes users actually land on; the backend authorizes by token so there's no data leak, only a possible 404 on a deep-linked detail URL). Address when workspace-delete ships (then `403 → redirect-to-dashboard` is the right handling), or it's mooted by M102's session-claim retiring the cookie.
- Architecture consult (Indy, Jun 24 2026): the deterministic end-state is workspace_id as a first-class **session/JWT claim** (Clerk active-org pattern), written on signup + `setActive` switch, enforced by middleware, read by the backend principal — so a logged-in user *always* carries it (true invariant, one source, no cookie/fallback). Decision: **ship M101's cookie/claim-hint design now** (same 0-RTT hot path; claim-reading code carries forward), then **M102 (AUTH spec)** establishes the session-claim invariant and removes the cookie + `withWorkspaceScope` fallback. M101's cookie/fallback layer is transitional, not throwaway — the resolver + route shape persist; only the cookie branch retires.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs Test Specification | Clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs spec, Failure Modes, Invariants, `dispatch/write_ts_adhere_bun.md` | All findings dispositioned |
| After `gh pr create` | `/review-pr` | Review-comments the open PR | Comments addressed |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `bun run test` | 1044 passed (0 failed) — incl. resolver A–G + fallback (§1/§2), billing dedup (§4), `tests/workspace-fetch-audit.test.ts` (audit mechanism), and 17 new §3 shell-streams + §5 island tests | ✅ |
| Resolver tests | `bun run test tests/workspace.test.ts` | covered within the full run (resolver precedence + fallback + `test_resolver_prefers_cookie_no_fetch` fetch-spy-not-called = Invariant 3) | ✅ |
| Build | `bun run build` | Compiled successfully (Next 16.2.9 / Turbopack), TypeScript clean, EXIT 0 | ✅ |
| e2e (audit) | `AGENTSFLEET_E2E_AUDIT=1 bun run test:e2e:acceptance` | **VERIFY GATE: test:e2e:acceptance skipped per environment constraint** — needs a local dev server with `AGENTSFLEET_E2E_AUDIT=1` (the audit endpoint is env-gated, so deployed dev/prod can't serve it), a reachable backend (`NEXT_PUBLIC_API_URL`), Clerk creds (`CLERK_SECRET_KEY`/`CLERK_WEBHOOK_SECRET` in the worktree `.env`) for `globalSetup` sign-in — none present in this frontend worktree (only Postgres is up). The audit *mechanism* is unit-tested; the 0-RTT hint path is proven by `test_resolver_prefers_cookie_no_fetch`. **Run in CI / a provisioned dev env before merge.** | ⏳ env-gated |
| §5.3 live QA | `/design-review` + browse on `/fleets/[id]` | **VERIFY GATE: skipped per environment constraint** — needs the running authenticated app + a real streaming fleet. Static guard (`test_fleetthread_uses_design_tokens`) passes. **Run in a provisioned dev env before merge.** | ⏳ env-gated |
| Lint + typecheck | `bun run lint` · `bun run typecheck` | oxlint --type-aware EXIT 0 · tsc --noEmit EXIT 0 | ✅ |
| Gitleaks | `gitleaks detect` | 2900 commits scanned, no leaks found | ✅ |
| 350-line gate | `git diff … awk` | All NEW files < 350 (5 shims ≤25L, `island-dynamic.test.ts` ~210L). Pre-existing `fleets-routes.test.ts` 508→519L (TS test file; the enforced FLL line-gate is Zig-only — not split, out of frontend-perf scope) | ✅ (new files) |

---

## Out of Scope

- New backend endpoints `GET …/fleets/{id}` and fleet-status summary → **M101_002** (own AUTH/greptile review; kills the `getFleet` O(100) scan and the `StatusTiles` client-side count rollup).
- `useOptimistic`/`useActionState` on every lifecycle button, View Transitions, PPR → follow-up UI polish spec.
- Persisting active workspace into the session JWT → dedicated AUTH spec (token-minting boundary).
- Client-side workspace caching across navigations (router-cache layer).
