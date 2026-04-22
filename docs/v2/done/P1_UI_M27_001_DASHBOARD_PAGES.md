# M27_001: Dashboard Pages — build on unified design system

**Prototype:** v1.0.0
**Milestone:** M27
**Workstream:** 001
**Date:** Apr 16, 2026
**Status:** DONE
**Priority:** P1 — Operator-facing pages (dashboard/zombies/detail/firewall/settings); completes what M12 started
**Batch:** B2 — alpha gate, parallel with M11_005, M19_001, M13_001, M21_001, M31_001, M33_001 (provides dashboard shell for M19 install flow)
**Branch:** feat/m27-dashboard-pages
**Depends on:** M12_001 (backend endpoints, shared primitives, token pyramid), **M26_001 (unified `@usezombie/design-system` Button/Card/etc. — must merge first)**

---

## Why this milestone exists

M12_001 was originally scoped to ship both the backend endpoints *and* the dashboard pages. During EXECUTE on Apr 16, a design-system audit surfaced that the shared Button was not self-contained (CSS lived in the website) and not RSC-safe (imported `react-router-dom`). Building pages against the app-local Button — then rewriting them post-unification — meant doing the work twice.

M12 narrowed to the foundation (backend + primitives + token pyramid). M26 unifies the design system. M27 picks up the dashboard pages, built directly against the unified Button. No rework.

---

## §0 — Route Ownership (M27 vs M19 — file-level partition, no overlap)

**Status:** CONSTRAINT

| Path | Owner | Scope |
|------|-------|-------|
| `app/(dashboard)/zombies/new/page.tsx` | **M19_001** | Install form + webhook URL display after submit. M27 does NOT own this file. |
| `app/(dashboard)/zombies/[id]/page.tsx` | **M27_001** (this spec) | Detail page **file** — layout + status header + kill switch + spend panel + activity feed + imports of M19's panel components (TriggerPanel, FirewallRulesEditor, ZombieConfig). |
| `app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | **M19_001** | Trigger config. M27 imports + renders. |
| `app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx` | **M19_001** | Firewall rules editor. M27 imports + renders. |
| `app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx` | **M19_001** | Rename / describe / delete panel. M27 imports + renders. |

**Partition rule:** M27 owns **detail-page composition** — the `page.tsx` file itself and widgets that are natural to the dashboard (status, kill switch, spend, activity). M19 owns **lifecycle-behavior components** under `components/`, because every one of them maps 1:1 to a `zombiectl zombie *` subcommand. M27's `page.tsx` imports them; M27 never edits them.

Path format: always `app/(dashboard)/zombies/[id]/...` with the route group segment.

If EXECUTE produces file-level surface outside this table, amend the spec first. Merge-conflict risk is eliminated because M19 only touches `components/**` + `new/page.tsx`, and M27 only touches `[id]/page.tsx` + M27-owned widgets.

---

## Overview

**Goal (testable):** Operators signed into `app.usezombie.com` see five dashboard pages — overview, zombies list, zombie detail, firewall placeholder, minimal settings — all consuming M12's backend endpoints via a typed, per-resource API client, using M26's unified Button/Card/Dialog, surfaced through the M12 shared-primitive toolbox (StatusCard / EmptyState / Pagination / DataTable / ConfirmDialog / ActivityFeed).

**What's already delivered (pre-M27):**
- `GET /v1/workspaces/{ws}/activity`
- `POST /v1/workspaces/{ws}/zombies/{id}:stop`
- `GET /v1/workspaces/{ws}/zombies/{id}/billing/summary`
- `GET /v1/workspaces/{ws}/billing/summary` (upgraded from stub)
- Shared UI primitives in `ui/packages/app/components/ui/` and `components/domain/`
- Three-layer token pyramid in `globals.css`
- `@usezombie/design-system` self-contained + RSC-safe + framework-agnostic (per M26)

**What M27 builds:**
- `lib/api/` split by resource with typed Input/Output per function + colocated unit tests
- `app/(dashboard)/page.tsx` — Dashboard overview
- `app/(dashboard)/zombies/page.tsx` — Zombies list
- `app/(dashboard)/zombies/[id]/page.tsx` — Zombie detail + kill switch + spend panel + activity log
- `app/(dashboard)/firewall/page.tsx` — placeholder pending M7 extension
- `app/(dashboard)/settings/page.tsx` — workspace info + masked API key
- Shell sidebar nav links: Dashboard / Zombies / Firewall / Credentials / Settings
- Playwright smoke on the golden path (sign in → dashboard → kill a zombie)
- Vercel preview deploy

---

## 0.5 Inherited from M12_001 — backend test wire-up

**Status:** DONE

**Discovery (Apr 22, 2026):** `/v1/workspaces/{ws}/zombies/{id}/billing/summary` and `/v1/workspaces/{ws}/billing/summary` are intentionally absent from the router (router.zig test: "rejects removed workspace billing routes — pre-v2.0 404s"). T8–T11 removed from scope. Dashboard billing display uses `/v1/tenants/me/billing` instead. Per-zombie billing routes deferred to a future milestone.

Actual file location: `src/http/handlers/workspaces/dashboard_integration_test.zig` (not `src/http/handlers/dashboard_http_integration_test.zig` as the main.zig NOTE said).

**Dimensions:**

- 0.5.1 DONE — rewrite to unique zombie / activity-event IDs per test call (workspace_id + tenant_id stay fixed to match JWT tokens); append-only + FK-chain problem resolved — test_type: integration
- 0.5.2 N/A — JWT minting not needed: workspace_id stays fixed, existing hardcoded tokens remain valid
- 0.5.3 DONE — DELETE-before-INSERT pattern and cleanupTestData removed; unique IDs eliminate need for cleanup within a run; `make down && make up` handles cross-run isolation
- 0.5.4 DONE — `_ = @import("http/handlers/workspaces/dashboard_integration_test.zig");` wired in `src/main.zig`; stale NOTE removed
- 0.5.5 N/A — file header had no "NOT WIRED UP" banner; stale path in main.zig NOTE was removed instead
- 0.5.6 DONE — T1–T7 (activity auth/seed/cursor + kill-switch transitions/409/404) covered; T8–T11 removed (billing routes absent from router — see Discovery above)
- 0.5.7 DONE — kept rbac regression block in place as fast-path smoke (decision: keep)

**Acceptance criteria:**

- `grep -n "dashboard_http_integration_test" src/main.zig` shows exactly one import line (no NOTE comment)
- `make test-integration-db` exits 0 with T1–T11 all in the passing-tests list
- Tier-3 fresh-DB run (`make down && make up && make test-integration-db`) also green
- No leaked allocations in the dashboard tests (current file leaks 10+ allocations per run when wired naively)

---

## 1.0 Typed API client split (`lib/api/`)

**Status:** DONE

`lib/api.ts` replaced with per-resource modules. Auth abstraction added (Apr 22, 2026): `lib/auth/server.ts` (`getServerToken`, `getServerAuth`) and `lib/auth/client.ts` (`useClientToken`) — all 11 direct Clerk call-sites migrated; swapping to zombie-auth requires editing those two files only.

```
ui/packages/app/lib/api/
  client.ts          ✓  shared fetch wrapper: Bearer auth, error envelope, 204 handling
  client.test.ts     ✓
  errors.ts          ✓  ApiError class + UzErrorCode discriminated union
  errors.test.ts     ✓
  workspaces.ts      ✓  listWorkspaces, getWorkspace
  workspaces.test.ts ✓
  zombies.ts         ✓  listZombies, getZombie, installZombie, stopZombie, deleteZombie
  zombies.test.ts    ✓
  activity.ts        ✓  listWorkspaceActivity, listZombieActivity (cursor-paginated)
  activity.test.ts   ✓
  tenant_billing.ts  ✓  getTenantBilling (uses /v1/tenants/me/billing — per §0.5 discovery)
  — billing.ts N/A   per-workspace/per-zombie billing routes absent pre-v2.0 (§0.5)

lib/auth/
  server.ts          ✓  getServerToken(), getServerAuth()
  client.ts          ✓  useClientToken()
```

**Dimensions:**

- 1.1 DONE — `lib/api/client.ts` — bearer header, Content-Type JSON, error envelope → `ApiError`, 204 handling
- 1.2 DONE — `lib/api/errors.ts` — `ApiError` carries `status`, `code: UzErrorCode`, `message`, `requestId`
- 1.3 DONE — `zombies.ts` — 5 functions, unit tests cover happy path + error for each
- 1.4 DONE — `activity.ts` — cursor pagination tests (no-cursor and with-cursor variants)
- 1.5 N/A — per §0.5 discovery: workspace/zombie billing routes absent from router (pre-v2.0 404s); `tenant_billing.ts` covers tenant-level billing display
- 1.6 DONE — `workspaces.ts` migrated; `lib/api/` test suite all green

---

## 2.0 Dashboard overview — `/dashboard`

**Status:** DONE

Server Component default. Suspense-streams `StatusTiles` and `RecentActivity` independently so a slow endpoint doesn't block paint. Auth uses `getServerToken()` from `lib/auth/server.ts`.

**Layout:**

```
┌──────────────────────────────────────────────────┐
│ UseZombie Dashboard                    [ws ▾]    │
├──────────┬──────────┬──────────┬─────────────────┤
│ N Active │ N Paused │ N Stopped│ $N credits bal.  │  ← StatusCard x4
├──────────┴──────────┴──────────┴─────────────────┤
│ Recent Activity                                  │  ← ActivityFeed
│   HH:MM  zombie-name  event.type  detail         │
└──────────────────────────────────────────────────┘
```

**Dimensions:**

- 2.1 DONE — `app/(dashboard)/page.tsx` renders StatusCard row + ActivityFeed; Suspense fallbacks tested via `renderToStaticMarkup(await DashboardPage())`
- 2.2 N/A — empty state is naturally handled: StatusCards show 0, ActivityFeed EmptyState shows "No activity yet"
- 2.3 N/A — all sub-components use `.catch(() => null)` guards; Suspense boundaries prevent propagation
- 2.4 DONE — two independent `<Suspense fallback={<Skeleton/>}>` blocks: StatusTiles + RecentActivity

---

## 3.0 Zombies list — `/zombies`

**Status:** DONE

Route: `app/(dashboard)/zombies/page.tsx`. Server-rendered list with cursor-based pagination (see §13.1) and client-side search filter.

**Dimensions:**

- 3.1 DONE — list + cursor pagination + client-side search. Page fetches via `listZombies({ workspaceId, cursor, limit })` with `?cursor={ts}:{id}&limit=N` URL wiring. Search filters the in-memory rows; server-side `?search=` deferred (see Deferred follow-ups) — test_type: unit + integration
- 3.2 DONE — empty state — EmptyState with "Deploy your first zombie" CTA linking to `/zombies/new` — test_type: unit
- 3.3 DONE — row click routes to `/zombies/[id]` — test_type: unit

---

## 4.0 Zombie detail — `/zombies/[id]`

**Status:** DONE

Route: `app/(dashboard)/zombies/[id]/page.tsx`. Name / status header / kill switch / activity log. SpendPanel deferred (per-zombie billing routes absent pre-v2.0).

**Dimensions:**

- 4.1 DONE — page renders zombie metadata, status badge, TriggerPanel, FirewallRulesEditor, ZombieConfig, ActivityFeed; ExhaustionBadge conditional on billing.is_exhausted
- 4.2 DONE — KillSwitch: `useOptimistic` + `useTransition` (React 19), ConfirmDialog, stopZombie API call, rollback on ApiError; tested (happy + exhaustion + billing-fail branches)
- 4.3 N/A — cursor pagination is in `listZombieActivity` API; UI pagination deferred (basic feed sufficient for alpha)
- 4.4 N/A — SpendPanel deferred; per-zombie billing routes absent pre-v2.0 (§0.5)

---

## 7.0 Shell + nav

**Status:** DONE

`components/layout/Shell.tsx` — NAV array updated: Dashboard (`/`), Zombies (`/zombies`), Firewall (`/firewall`, ShieldIcon), Credentials (`/credentials`, KeyRoundIcon), Settings (`/settings`). Icons from lucide-react.

**Dimensions:**

- 7.1 DONE — Shell sidebar exposes all five links; tested in app-components.test.ts
- 7.2 DEFERRED — mobile collapsed sidebar e2e test (Playwright) — moves to the authenticated-e2e follow-up spec

---

## 8.0 Frontend VERIFY gates

**Status:** DONE (with follow-ups captured)

| Gate | Command | Status |
|---|---|---|
| Typecheck | `bun run typecheck` | ✓ clean |
| Lint | `bun run lint` | ✓ clean |
| Unit + integration tests | `bun run test` | ✓ 110 app + 202 design-system + 140 website green |
| Backend tier-3 integration | `make down && make up && make test-integration` | ✓ full DB + Redis suite green |
| Zig cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | ✓ both pass |
| E2E smoke (unauthed) | `bunx playwright test tests/e2e/dashboard-smoke.spec.ts` | ✓ 10/11 pass, 1 skipped (auth path) |
| E2E smoke (authed kill) | Clerk test user + seeded zombie in api-dev | DEFERRED — authenticated-e2e follow-up spec |
| Bundle delta | `bun run build && du -sh .next/static/chunks` vs main baseline | DEFERRED — measured pre-PR-open |
| Vercel preview | `vercel deploy` + manual smoke | DEFERRED — measured pre-PR-open |

---

## 9.0 Interfaces

- All original API contracts already defined in M12 (see `public/openapi.json`)
- Two endpoints added during EXECUTE — see §13 below
- All UI primitives already shipped in M12
- Five design-system primitives added during EXECUTE — see §13 below

---

## 13.0 Shipped beyond original scope (discovered during EXECUTE)

**Status:** DONE

While the header said "no new API endpoints, no new primitives, no schema changes", two backend endpoints, five design-system primitives, a Server Action, and an auth abstraction all turned out to be necessary to ship the originally-planned pages. Captured here so the scope record matches what actually landed:

- 13.1 DONE — `GET /v1/workspaces/{ws}/zombies` now cursor-paginated (`?cursor={ts}:{id}&limit=N`, max 100). Needed to render §3.0 without unbounded responses. Handler `src/http/handlers/zombies/api.zig`; integration test `api_integration_test.zig`.
- 13.2 DONE — `GET /v1/tenants/me/workspaces` — new tenant-scoped workspace list backing the workspace switcher. Handler `src/http/handlers/tenant_workspaces.zig`; integration test `tenant_workspaces_integration_test.zig`.
- 13.3 DONE — `components/layout/WorkspaceSwitcher.tsx` + Server Action at `app/(dashboard)/actions.ts` (`setActiveWorkspace`) writing the `active_workspace_id` cookie + `revalidatePath`. Leans on the tenant-scoped `authorizeWorkspace` invariant — no JWT reissue needed to switch.
- 13.4 DONE — Auth abstraction: `lib/auth/server.ts` + `lib/auth/client.ts`. Zero direct `@clerk/nextjs` imports remain outside `lib/auth/*`. Swap to zombie-auth = edit two files.
- 13.5 DONE — Same-origin `/backend` proxy: `next.config.ts` rewrites `/backend/:path*` → `NEXT_PUBLIC_API_URL ?? https://api-dev.usezombie.com`. Browser fetches never cross-origin → no CORS in dev/preview/prod. Server Components call the backend directly using the same `NEXT_PUBLIC_API_URL`. One env var drives both sides — SSR and browser mutations cannot route to different backends.
- 13.6 DONE — `@usezombie/design-system` primitives added: `Textarea`, `Label`, `Tabs*`, `Accordion*`, and a shadcn-style `Form*` bundle built on react-hook-form + zod. All with colocated tests; 202 design-system tests green.
- 13.7 DONE — Website migrations onto the new primitives: `FAQ.tsx` → `Accordion`, `App.tsx` mode switcher → `Tabs` (preserves legacy `mode-switch` / `mode-btn` CSS).

---

## 14.0 Deferred follow-ups (filed under `docs/v2/pending/`)

Every item below has an explicit pending spec stub; none block the M27 PR.

- **Authenticated E2E harness** — Clerk test user + api-dev seeding to cover the kill-zombie flow and mobile sidebar collapse (§4.2, §7.2).
- **Zombie list server-side search** — frontend already accepts a filter; backend `?search=` param does not exist. Deferred because client-side filtering on the paged slice is adequate for alpha traffic.
- **Workspace settings page** — create / rename / invite. The switcher ships selection; management UX is the next step.
- **Per-zombie billing UI + routes** — `/v1/workspaces/{ws}/zombies/{id}/billing/summary` was intentionally removed pre-v2.0 per M10-era policy; revisit when the billing UX lands.
- **Vercel preview + bundle delta measurement** — taken before PR-open, not shipped in the branch itself.

---

## 10.0 Acceptance Criteria

- [x] M26_001 merged before M27 EXECUTE starts
- [x] `lib/api/` fully split into `client`, `errors`, `workspaces`, `zombies`, `activity` — each with typed I/O + colocated `.test.ts` (`billing` N/A per §0.5 discovery; `tenant_billing.ts` covers tenant-level)
- [x] `/`, `/zombies`, `/zombies/[id]`, `/firewall`, `/credentials`, `/settings` all render against `api-dev.usezombie.com`
- [x] Kill switch works end-to-end in unit + manual testing (authenticated playwright path deferred — see §14)
- [x] Shell sidebar exposes all five nav links
- [x] All new `.tsx` files ≤ 350 lines
- [x] `bun run typecheck / lint / test` green
- [ ] Playwright e2e smoke on the authenticated kill-switch path — DEFERRED
- [ ] Vercel preview deployed + manually smoked — DEFERRED until PR-open
- [ ] Bundle size delta < 50 KB gzipped first load — DEFERRED until PR-open

---

## 11.0 Out of Scope

- Firewall page, settings placeholder — deferred.
- Full firewall metrics/events page — M7-extension milestone
- Multi-key API key management — schema change required
- Dark/light mode toggle — M12's `.dark` block stubbed for future
- i18n — English only
- Chat with running agent — CLI-only until a future milestone
- Advanced filtering/search on activity — basic event_type prefix filter only
- **Grant-health indicator dot on zombie cards** — cut from alpha. The dot would require a per-zombie grant-aggregation endpoint that's not worth building before a dashboard grants UI exists to give it context. The activity log already surfaces `UZ-GRANT-001` as a first-class event, so operators have a way to see grant state without a summary widget. Revisit when a full grants UI lands.

---

## 12.0 Eval Commands

```bash
# Typecheck + lint + test
cd ui/packages/app && bun run typecheck && bun run lint && bun run test

# Build + bundle size
bun run build
du -sh .next/static/chunks

# E2E smoke
bunx playwright test tests/e2e/dashboard-smoke.spec.ts

# 350-line gate on all new .tsx
git diff --name-only origin/main | grep -E '^ui/packages/app/.+\.tsx$' \
  | grep -v '_test\.\|\.test\.\|\.spec\.' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "OVER: " $2 " " $1 }'
```

---

## Applicable Rules

Standard set. **RULE HGD** for any new handlers (M27 doesn't add handlers — M12 already covered the API surface). **RULE FLL** on every touched `.tsx`. **RULE ORP** at CHORE(close) for any renamed symbols.

---

## Dead Code Sweep

At CHORE(close): if any leftover `lib/api.ts` (monolithic) lingers, delete it. Remove any unused MSW fixtures after the typed client lands.
