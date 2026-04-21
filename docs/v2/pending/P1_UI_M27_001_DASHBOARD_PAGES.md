# M27_001: Dashboard Pages — build on unified design system

**Prototype:** v1.0.0
**Milestone:** M27
**Workstream:** 001
**Date:** Apr 16, 2026
**Status:** PENDING — blocked on M26_001
**Priority:** P1 — Operator-facing pages (dashboard/zombies/detail/firewall/settings); completes what M12 started
**Batch:** B2 — alpha gate, parallel with M19_001, M13_001, M21_001, M11_005 (provides dashboard shell for M19 create-zombie flow)
**Branch:** feat/m27-dashboard-pages (not yet created)
**Depends on:** M12_001 (backend endpoints, shared primitives, token pyramid), **M26_001 (unified `@usezombie/design-system` Button/Card/etc. — must merge first)**

---

## Why this milestone exists

M12_001 was originally scoped to ship both the backend endpoints *and* the dashboard pages. During EXECUTE on Apr 16, a design-system audit surfaced that the shared Button was not self-contained (CSS lived in the website) and not RSC-safe (imported `react-router-dom`). Building pages against the app-local Button — then rewriting them post-unification — meant doing the work twice.

M12 narrowed to the foundation (backend + primitives + token pyramid). M26 unifies the design system. M27 picks up the dashboard pages, built directly against the unified Button. No rework.

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

**Status:** PENDING

M12_001 shipped `src/http/handlers/dashboard_http_integration_test.zig` (the T1–T11 HTTP integration tests against the three dashboard-backing endpoints) but the file is deliberately **not imported** in `src/main.zig` (see the NOTE beside the telemetry import there). Wiring it up surfaces real bugs: `seedTestData` deletes from `core.activity_events`, which is append-only at the DB layer, and the resulting foreign-key chain (`activity_events → zombies → workspaces → tenants`) blocks cleanup of parent rows too. Each test currently uses shared fixed IDs so the second test's seed conflicts with the first test's leftover state and T5–T11 fail on tier-2.

**Interim regression coverage (landed in M12_001 PR #221):** `src/http/rbac_http_integration_test.zig` was extended with two negative-assertion tests — `TEST_USER_TOKEN` on `:stop` and on per-zombie billing summary both assert 403 + `ERR_INSUFFICIENT_ROLE`. That locks RULE BIL on those handlers in CI today. It does NOT cover positive-path assertions (operator succeeds, IDOR 404, etc.) or the four non-billing dashboard tests (T1–T4 workspace activity).

**M27_001 closes this gap.** The frontend pages need a working backend-integration test suite they can lean on (for example, the kill-switch Playwright path needs backend tests green end-to-end, not just the RBAC guard). Scope in this workstream:

**Dimensions:**

- 0.5.1 PENDING — rewrite `dashboard_http_integration_test.zig` to use unique workspace_id / tenant_id / zombie_ids per test run (UUIDv7 generated at test start, not hardcoded constants), so the append-only + FK-chain cleanup problem disappears — test_type: integration
- 0.5.2 PENDING — add a test-signing-key helper (`src/http/test_tokens.zig` or equivalent) that mints RS256 JWTs from a test keypair embedded in the test binary, so TOKEN_USER / TOKEN_OPERATOR can be generated per-run matching the per-run workspace_id — replaces the current pasted-JWT-literal pattern — test_type: unit
- 0.5.3 PENDING — delete the DELETE-before-INSERT pattern from `seedTestData` / `cleanupTestData`; rely on `make down && make up` + `_reset-test-db` for cross-run isolation; within a run, unique IDs per test remove the need to clean — test_type: integration
- 0.5.4 PENDING — uncomment the `_ = @import("http/handlers/dashboard_http_integration_test.zig");` line in `src/main.zig` and remove the NOTE explaining why it's absent — test_type: build
- 0.5.5 PENDING — remove the "⚠ NOT WIRED UP" banner from the top of `dashboard_http_integration_test.zig` — test_type: lint
- 0.5.6 PENDING — `make test-integration-db` fails if any of T1–T11 fails (tier-3 verified: `make down && make up && make test-integration-db`) — test_type: integration
- 0.5.7 PENDING — optional: remove the M12 regression-coverage block from `rbac_http_integration_test.zig` once the dashboard integration tests are green, OR keep it as a fast-path smoke (the rbac test runs in a few hundred ms; the dashboard test spins up a full server and is slower). Decide during EXECUTE — test_type: design

**Acceptance criteria:**

- `grep -n "dashboard_http_integration_test" src/main.zig` shows exactly one import line (no NOTE comment)
- `make test-integration-db` exits 0 with T1–T11 all in the passing-tests list
- Tier-3 fresh-DB run (`make down && make up && make test-integration-db`) also green
- No leaked allocations in the dashboard tests (current file leaks 10+ allocations per run when wired naively)

---

## 1.0 Typed API client split (`lib/api/`)

**Status:** PENDING

Today `ui/packages/app/lib/api.ts` is a single flat file (~80 lines). M27 replaces it with per-resource modules, each exporting strictly-typed Input / Output per function and one colocated unit test file (`*.test.ts`) covering URL construction, request body, response parsing, and error mapping (401, 404, 409, 500 → typed `ApiError`).

```
ui/packages/app/lib/api/
  client.ts          # shared fetch wrapper: Bearer auth, error envelope, x-request-id header
  client.test.ts
  errors.ts          # ApiError class + UZ-* code constants
  errors.test.ts
  workspaces.ts      # listWorkspaces, getWorkspace (existing, migrated)
  workspaces.test.ts
  zombies.ts         # listZombies, getZombie, stopZombie, deleteZombie
  zombies.test.ts
  activity.ts        # listWorkspaceActivity, listZombieActivity (cursor-paginated)
  activity.test.ts
  billing.ts         # getWorkspaceBillingSummary, getZombieBillingSummary
  billing.test.ts
```

**Dimensions:**

- 1.1 PENDING — target: `lib/api/client.ts` — input: `request<Input, Output>(path, method, body?, token)` — expected: bearer header set when token present, Content-Type JSON, error envelope → `ApiError`, propagates `x-request-id` for tracing — test_type: unit
- 1.2 PENDING — target: `lib/api/errors.ts` — input: `ApiError` class carries `status`, `code: UzErrorCode`, `message`, `request_id` — expected: instanceof + discriminated by `code` — test_type: unit
- 1.3 PENDING — `zombies.ts` — each of 4 functions has typed Input/Output + a unit test per happy + one error path
- 1.4 PENDING — `activity.ts` — cursor pagination round-trip test
- 1.5 PENDING — `billing.ts` — both scopes (workspace + zombie) and both `period_days` values tested
- 1.6 PENDING — `workspaces.ts` — existing functions migrated; existing `tests/api.test.ts` still passes (or is split per resource)

---

## 2.0 Dashboard overview — `/dashboard`

**Status:** PENDING

Server Component default. Suspense-stream each tile independently so a slow endpoint doesn't block paint.

**Layout:**

```
┌──────────────────────────────────────────────────┐
│ UseZombie Dashboard                    [ws ▾]    │
├──────────┬──────────┬──────────┬─────────────────┤
│ N Active │ N Paused │ N Stopped│ $N / 7d spend   │  ← StatusCard x4
├──────────┴──────────┴──────────┴─────────────────┤
│ Recent Activity (last 50)                        │  ← ActivityFeed
│   HH:MM  zombie-name  event.type  detail         │
│   ...                                   [See all]│
└──────────────────────────────────────────────────┘
```

**Dimensions:**

- 2.1 PENDING — target: `app/(dashboard)/page.tsx` — input: workspace with 3 active + 1 paused + 0 stopped — expected: StatusCard row with correct counts, spend tile, activity feed — test_type: integration (MSW mock via `vi.stubGlobal("fetch")`)
- 2.2 PENDING — empty state — workspace with zero zombies — expected: EmptyState with copy "Deploy your first zombie" — test_type: unit
- 2.3 PENDING — error state — API returns 500 on one tile — expected: tile renders error toast, other tiles still render — test_type: unit
- 2.4 PENDING — Suspense streaming — each tile has its own `<Suspense fallback={<Skeleton/>}>` — test_type: build (React Server Components route analysis)

---

## 3.0 Zombies list — `/zombies`

**Status:** PENDING

Route: `app/(dashboard)/zombies/page.tsx`. DataTable + Pagination with status, last_active, spend_7d columns. Search param wired through to `GET /v1/workspaces/{ws}/zombies?search=`.

**Dimensions:**

- 3.1 PENDING — list + pagination + search — expected: DataTable rows, `page=2&limit=20` URL wiring, search debounced 300ms — test_type: integration
- 3.2 PENDING — empty state — no zombies — EmptyState with "Deploy your first zombie" action — test_type: unit
- 3.3 PENDING — row click → routes to `/zombies/[id]` — test_type: unit

---

## 4.0 Zombie detail — `/zombies/[id]`

**Status:** PENDING

Route: `app/(dashboard)/zombies/[id]/page.tsx`. Name / status / uptime / 7d + 30d spend panel / kill switch / cursor-paginated activity log filtered by event_type.

**Dimensions:**

- 4.1 PENDING — page renders with zombie metadata + spend + activity — test_type: integration
- 4.2 PENDING — KillSwitch — uses M12's ConfirmDialog primitive + `useOptimistic` for instant status flip + rollback on 409 — test_type: unit + e2e
- 4.3 PENDING — ActivityLog — cursor pagination + event_type prefix filter — test_type: unit
- 4.4 PENDING — SpendPanel — both `?period_days=7` and `?period_days=30` rendered, zero-state handled — test_type: unit

---

## 7.0 Shell + nav

**Status:** PENDING

Route: `components/layout/Shell.tsx` — append sidebar nav links for Dashboard / Zombies / Firewall / Credentials / Settings. Credentials route goes to a page M13 will ship; for M27 it's a placeholder.

**Dimensions:**

- 7.1 PENDING — Shell sidebar exposes all five links — test_type: unit
- 7.2 PENDING — mobile collapsed sidebar still routes correctly — test_type: e2e

---

## 8.0 Frontend VERIFY gates

**Status:** PENDING

| Gate | Command |
|---|---|
| Typecheck | `bun run typecheck` |
| Lint | `bun run lint` |
| Unit + integration tests | `bun run test` (all M12 + M27 tests green) |
| E2E smoke | `bunx playwright test` — sign in → dashboard → kill zombie happy path |
| Bundle delta | first-load gzipped delta < 50 KB |
| Build | `bun run build` clean |
| Vercel preview | `vercel deploy` succeeds; manual smoke against preview URL |

---

## 9.0 Interfaces

- All API contracts already defined in M12 (see `public/openapi.json`)
- All UI primitives already shipped in M12
- All design-system components already unified in M26

M27 is purely composition on top of what M12 + M26 delivered. No new API endpoints, no new primitives, no schema changes.

---

## 10.0 Acceptance Criteria

- [ ] M26_001 merged before M27 EXECUTE starts
- [ ] `lib/api/` fully split into `client`, `errors`, `workspaces`, `zombies`, `activity`, `billing` — each with typed I/O + colocated `.test.ts`
- [ ] `/dashboard`, `/zombies`, `/zombies/[id]`, `/firewall`, `/settings` all render against `api-dev.usezombie.com`
- [ ] Kill switch works end-to-end (click → confirm → status=stopped → toast)
- [ ] Shell sidebar exposes all five nav links, inherits mobile responsive behavior
- [ ] All new `.tsx` files ≤ 350 lines
- [ ] `bun run typecheck / lint / test / build` green
- [ ] Playwright e2e smoke green on the kill-switch path
- [ ] Vercel preview deployed + manually smoked
- [ ] Bundle size delta < 50 KB gzipped first load

---

## 11.0 Out of Scope

- Firewall page, settings placeholder — deferred.
- Full firewall metrics/events page — M7-extension milestone
- Multi-key API key management — schema change required
- Dark/light mode toggle — M12's `.dark` block stubbed for future
- i18n — English only
- Chat with running agent — CLI-only until a future milestone
- Advanced filtering/search on activity — basic event_type prefix filter only

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
