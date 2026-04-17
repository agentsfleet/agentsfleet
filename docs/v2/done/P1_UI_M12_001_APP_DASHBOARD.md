# M12_001: Operator Dashboard — API + Shared UI Foundation

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 001
**Date:** Apr 10, 2026 (amended Apr 14 + Apr 16, 2026)
**Status:** DONE — scope narrowed on Apr 16; pages moved to M27
**Priority:** P1 — Operator-facing surface; first web UI for non-CLI users
**Batch:** B5 — after M8 (Slack plugin creates workspaces); M11_003 (invite signup) ships alongside this batch
**Branch:** feat/m12-app-dashboard
**Worktree:** /Users/kishore/Projects/usezombie-m12-app-dashboard
**Depends on:** M4_001 (approval gate), M2_001 (activity stream API), **M24_001 (REST workspace-scoped route refactor — must land first)**
**Blocks:** M26_001 (Design System Unification), then M27_001 (Dashboard Pages)

**Extended by (post-M12 milestones that build on this shell):**
- M13_001 (B5): Credential Vault UI — full vault management; supersedes §4.0 of this spec
- M19_001 (B6): Zombie Lifecycle UI — create, trigger config, firewall rules; supersedes "out of scope: configuration editing"
- M20_001 (B6): Approval Inbox — pending badge on zombie cards, /approvals page, Pending tab on zombie detail
- M21_001 (B6): BYOK Provider — Provider tab on Settings page (§4.0 extended)
- M22_001 (B7): Integration Grants UI — Integrations tab on zombie detail
- M11_003 (B5): Invite Code + Signup — entry point that lands users on this dashboard

---

## Amendment note — Scope pivot (Apr 16, 2026)

Mid-EXECUTE, a review of the frontend workstream surfaced that the `@usezombie/design-system` package was **not self-contained** — its `Button.tsx` emits `.z-btn` class names, but the actual CSS rules live in the consumer (`ui/packages/website/src/styles.css`). The design-system components also import `react-router-dom`, which is incompatible with Next.js Server Components.

Building the dashboard pages against the app-local `components/ui/button.tsx` — then migrating them to a unified Button post-hoc — would mean writing the pages twice. The user called this: *"I prefer that we ship the Design system M26 first, that is important than finishing up the work."*

**Pivot:**

1. **M12 ships narrowed** to what's already delivered: three new backend endpoints (workspace activity, zombie :stop, per-zombie billing summary), OpenAPI entries, HTTP integration tests, shared UI primitives (StatusCard / EmptyState / Pagination / DataTable / ConfirmDialog / ActivityFeed), and the three-layer token pyramid in `globals.css`. All ✅ green at commit time.
2. **M26_001 Design System Unification** becomes the next active milestone. Makes the design-system package self-contained (CSS + components + tokens), framework-agnostic via Radix `Slot` + `asChild`, and RSC-safe. Migrates both `ui/packages/app` **and** `ui/packages/website` to the unified package.
3. **M27_001 Dashboard Pages** is blocked by M26 and picks up where M12 stopped: typed API client split by resource, `/dashboard` / `/zombies` / `/zombies/[id]` / `/firewall` / `/settings` pages, Shell nav, frontend VERIFY gates. Pages build on the unified Button landed in M26, so no rework.

## Amendment note (Apr 14, 2026)

The original spec (Apr 10) assumed workspace-scoped REST endpoints (`/v1/workspaces/{ws}/zombies/...`) already existed. A data/route audit on `main` (commit `a85ae78`) found the API was flat (`/v1/zombies/?workspace_id=`) and several consumed endpoints (kill switch, firewall metrics, api-keys) did not exist at all. This amendment:

1. **Split out REST URL refactor to its own milestone — M24_001.** Flat `/v1/zombies/*` → `/v1/workspaces/{ws}/zombies/*` refactor ships as PR #1 ahead of M12's UI work. Query params reserved for `page`, `limit`, `cursor`, `search`.
2. **Adds Tier A backend endpoints** that are cheap to build on existing schema (workspace activity, kill switch, per-zombie spend aggregation).
3. **Defers to follow-up milestones**: full firewall page (no backing data today), multi-key API key management (schema has single `api_key_hash` per tenant), trust score (not a real primitive yet).
4. **UI is built against mocked responses first, swapped to real endpoints as they land.** Lets frontend and backend iterate in parallel.

**PR strategy:** PR #1 = M24_001 REST refactor (mechanical, ships independently). PR #2 = M12_001 narrowed scope (backend + primitives + tokens), this spec. PR #3 = M26_001. PR #4 = M27_001 frontend pages.

---

## Overview

**Goal (testable):** `app.usezombie.com` shows operators the zombie-specific dashboard for the 3 canonical zombie use cases (Lead Collector, Hiring Agent, Ops Zombie — see `docs/nostromo/lead_collector_zombie.md`): running zombie status cards, workspace-wide activity stream, per-zombie spend from execution telemetry, kill switch, and navigation entries for future milestones' pages. All paths are REST-ful and workspace-scoped. The app consumes existing + a handful of new endpoints and authenticates via Clerk.

**Existing (`ui/packages/app`):** Next.js 16 App Router, Clerk auth, Shell layout with sidebar, workspace list/detail (v1), API client (`lib/api.ts`) with Bearer token pattern, Vitest + Playwright, PostHog.

**M12 adds:** REST URL refactor on the API side + 4 new frontend pages (+ 1 placeholder) layered into the existing `(dashboard)` route group.

**Solution summary (post Apr 16 pivot):**
- **API refactor (prerequisite, M24_001 — DONE):** flat `/v1/zombies/*` → workspace-scoped.
- **Backend adds (workstream 1, DONE this milestone):** `GET /v1/workspaces/{ws}/activity`, `POST /v1/workspaces/{ws}/zombies/{id}:stop`, `GET /v1/workspaces/{ws}/zombies/{id}/billing/summary`. OpenAPI entries + integration tests landed with them.
- **Shared UI foundation (workstream 2, DONE this milestone):** six app-local primitives (StatusCard, EmptyState, Pagination, DataTable, ConfirmDialog, ActivityFeed) + three-layer token pyramid in `globals.css` + existing `button.tsx` refactored to semantic tokens.
- **Frontend pages (MOVED_TO_M27):** `/dashboard`, `/zombies`, `/zombies/[id]`, `/settings`, `/firewall`, Shell nav, typed API client. Built against the unified Design System landed in M26.

---

## 1.0 REST Workspace-Scoped Route Refactor — MOVED to M24_001

Flat `/v1/zombies/*` → `/v1/workspaces/{ws}/zombies/*` refactor is now its own milestone (M24_001, PR #1). This spec depends on M24_001 landing first. Route migration table, dimensions, and verification live in the M24 spec.

---

## 2.0 Backend Endpoints Added

**Status:** IN_PROGRESS — handlers + unit tests DONE; openapi.json + integration tests PENDING

Three new endpoints backed by existing tables. Each handler ≤ 165 lines.

### 2.1 Workspace-wide activity stream

`GET /v1/workspaces/{ws}/activity?cursor=&limit=`

Backs dashboard "Recent Activity" feed. Uses existing `core.activity_events` table with its `idx_activity_events_workspace_created` index. Joins `core.zombies` for the display name.

Response:
```json
{
  "events": [
    { "id": "...", "zombie_id": "...", "zombie_name": "lead-collector",
      "event_type": "email.received", "detail": "jane@acme.com", "created_at": 1744... }
  ],
  "next_cursor": "base64:..."
}
```

- 2.1 DONE (handler + 6 unit tests; cursor pagination via `activity_stream.queryByWorkspaceOnConn` — RULE CNX single-conn) — INTEGRATION PENDING — target `src/http/handlers/workspace_activity.zig` — input: workspace with 3 zombies each with 20 events — expected: merged feed newest-first, cursor pagination

### 2.2 Kill switch

`POST /v1/workspaces/{ws}/zombies/{id}:stop`

Flips `core.zombies.status` from `active`/`paused` → `stopped`. `stopped` is the non-terminal halt state; a follow-up M19 action will re-start. `killed` remains the terminal marker (via DELETE). Returns 409 (`UZ-ZMB-010`) if already stopped/killed; 404 if zombie not in workspace (IDOR guard).

- 2.2 DONE (handler + 4 unit tests; writes `zombie_stopped` activity event; 409 `UZ-ZMB-010` registered; IDOR guard via `getZombieWorkspaceId`) — INTEGRATION PENDING — target `src/http/handlers/zombie_lifecycle.zig` — input: active zombie — expected: status=stopped after call, activity event recorded, 409 on re-call
- 2.3 DONE (handler path covered) — INTEGRATION PENDING — input: zombie not in workspace — expected: 404

### 2.3 Per-zombie billing summary

`GET /v1/workspaces/{workspace_id}/zombies/{zombie_id}/billing/summary?period_days=7|30`

Per-zombie slice of the workspace billing summary. **Uses the exact same response schema as `GET /v1/workspaces/{workspace_id}/billing/summary`** — only the scope (path) differs. This is the REST-correct shape per `docs/REST_API_DESIGN_GUIDELINES.md` §2 (reflect hierarchy in path) and §5 (resource IDs in path, not query params). SDK shape: `workspaces.billing.summary(ws)` vs `workspaces.zombies.billing.summary(ws, zombie)`.

Data source: aggregates `zombie_execution_telemetry.credit_deducted_cents` (same table `/billing/summary` already reads) filtered by `zombie_id`, windowed by `recorded_at`. A shared aggregator (`src/state/billing_summary_store.zig`) powers both the workspace and per-zombie handlers so they cannot drift.

Response matches the existing `billing/summary` envelope (`workspace_id`, `period_days`, `period_start_ms`, `period_end_ms`, `completed`, `non_billable`, `non_billable_score_gated`, `total_runs`, `total_cents`, `request_id`), plus a `zombie_id` field when called at the zombie scope.

**Side effect of landing this:** the pre-existing `GET /v1/workspaces/{ws}/billing/summary` stub (zeros since M10_001) is upgraded to read real data via the shared aggregator. No OpenAPI schema change — just behaviour.

- 2.4 DONE (handler + 5 unit tests; shared aggregator + 2 unit tests; workspace-summary handler upgraded + 3 new unit tests) — INTEGRATION PENDING — target `src/http/handlers/zombie_billing_summary.zig` — input: zombie with 47 telemetry rows in 7d — expected: response matches workspace summary shape, scoped to zombie
- 2.5 DONE (handler returns zeros with 200 for empty telemetry) — INTEGRATION PENDING — input: zombie with no telemetry — expected: zero counters, 200 (not 404)
- 2.6 DONE (IDOR guard via `getZombieWorkspaceId` + workspace-id match) — INTEGRATION PENDING — input: zombie_id not in workspace — expected: 404 `UZ-ZMB-009`

---

## 3.0 – 8.0 Dashboard pages + nav — MOVED_TO_M27

The five pages (`/dashboard`, `/zombies`, `/zombies/[id]`, `/firewall`, `/settings`), the Shell sidebar nav updates, and the typed API client split are now scoped to `docs/v2/pending/P1_UI_M27_001_DASHBOARD_PAGES.md`. M27 is blocked by M26_001 — pages consume the unified Button landed there.

The dimensions below (originally §3.x – §8.1) are preserved verbatim for reference by M27's author. Their status stays **MOVED_TO_M27**; the source of truth is the new spec.

---

## 3.0 Dashboard Page (Overview) — MOVED_TO_M27

**Status:** PENDING

Landing page after Clerk login. Route: `app/(dashboard)/page.tsx`.

**Layout:**
```
┌─────────────────────────────────────────────────┐
│ UseZombie Dashboard                    [ws ▾]   │
├──────────┬──────────┬──────────┬────────────────┤
│ 3 Active │ 1 Paused │ 0 Stopped│ $12.40 / 7d   │
├──────────┴──────────┴──────────┴────────────────┤
│ Recent Activity                                 │
│ 10:47 lead-collector: email.received jane@...   │
│ 10:47 lead-collector: crm.write hubspot contact │
│ 10:45 hiring-agent:  slack.post #hiring         │
│ ...                                    [See all]│
└─────────────────────────────────────────────────┘
```

Firewall summary bar deferred (no backend data). Spend tracker uses per-zombie spend, summed client-side across zombies for the workspace total.

**Dimensions:**
- 3.1 PENDING — target `app/(dashboard)/page.tsx` — input: workspace with 3 active + 1 paused zombies — expected: status cards correct, from `GET /v1/workspaces/{ws}/zombies` — test_type: integration (MSW mock)
- 3.2 PENDING — target `components/domain/ActivityFeed.tsx` — input: `GET /v1/workspaces/{ws}/activity?limit=50` — expected: event rows with timestamp + zombie name + event_type + detail — test_type: unit
- 3.3 PENDING — target `components/domain/SpendTracker.tsx` — input: `GET /v1/workspaces/{workspace_id}/billing/summary?period_days=7` — expected: workspace-wide total displayed, drill-down via per-zombie endpoint on hover — test_type: unit

---

## 4.0 Zombies List Page

**Status:** PENDING

Route: `app/(dashboard)/zombies/page.tsx`. Paged list of all zombies in the workspace with status, last activity timestamp, and 7d spend.

- 4.1 PENDING — target `app/(dashboard)/zombies/page.tsx` — input: `GET /v1/workspaces/{ws}/zombies?page=1&limit=20` — expected: table with id, name, status, last_active, spend_7d; pagination controls — test_type: integration
- 4.2 PENDING — same — input: `?search=lead` — expected: server-side filter applied — test_type: integration

---

## 5.0 Zombie Detail Page

**Status:** PENDING

Route: `app/(dashboard)/zombies/[id]/page.tsx`.

**Dimensions:**
- 5.1 PENDING — target page — input: zombie id — expected: name, status, trigger type, uptime, 7d spend, kill switch button — test_type: integration
- 5.2 PENDING — target `app/(dashboard)/zombies/[id]/components/KillSwitch.tsx` — input: click on active zombie — expected: confirm dialog → `POST /v1/workspaces/{ws}/zombies/{id}:stop` → status=stopped → toast — test_type: integration
- 5.3 PENDING — target `app/(dashboard)/zombies/[id]/components/ActivityLog.tsx` — input: zombie with 200 events — expected: cursor-paginated, filterable by event_type prefix — test_type: unit
- 5.4 PENDING — target `app/(dashboard)/zombies/[id]/components/SpendPanel.tsx` — input: 7d + 30d calls to `/zombies/{id}/billing/summary` — expected: both period values shown, run count, zero-state handled — test_type: unit

Trust chart (original 2.4) **removed** — trust score is not a real data primitive today. Re-scope when defined.

---

## 6.0 Firewall Page (placeholder)

**Status:** PENDING

Route: `app/(dashboard)/firewall/page.tsx`. Renders a `"Firewall metrics — coming soon"` placeholder with a link to the spec for the follow-up milestone. Sidebar nav link is wired so users discover the page exists.

Full firewall metrics/events surface deferred — no backing table or write path in `outbound_proxy.zig` today. Belongs in an M7-extension milestone.

- 6.1 PENDING — target `app/(dashboard)/firewall/page.tsx` — input: authenticated user — expected: placeholder renders, no API calls made — test_type: unit

---

## 7.0 Settings Page (minimal)

**Status:** PENDING

Route: `app/(dashboard)/settings/page.tsx`. Two cards:

1. **Workspace info** — name, id (copyable), plan tier from `core.workspace_entitlements` if queryable.
2. **Tenant API key** — shows the single `core.tenants.api_key_hash` **prefix + masked** (never the full value — it's a hash anyway), with a "last used" timestamp if tracked.

Multi-key management (generate new, revoke individual, name keys) **deferred** to a dedicated milestone — requires new `api_keys` table with rotation/revocation. The current schema has one hash per tenant.

- 7.1 PENDING — target `app/(dashboard)/settings/page.tsx` — input: workspace id — expected: workspace info card + masked API key display — test_type: integration

---

## 8.0 Auth + Layout + Nav

**Status:** DONE (pre-existing) + small diff

Clerk auth + Shell layout inherited as-is. M12 adds sidebar nav entries.

- 8.1 PENDING — target `components/layout/Shell.tsx` — input: authenticated user — expected: sidebar shows Dashboard, Zombies, Firewall, Credentials, Settings — test_type: unit

---

## 9.0 Interfaces

### 9.1 API endpoints consumed

**Existing (post-refactor):**
```
GET    /v1/workspaces/{ws}/zombies?page=&limit=&search=
POST   /v1/workspaces/{ws}/zombies
DELETE /v1/workspaces/{ws}/zombies/{id}
GET    /v1/workspaces/{ws}/zombies/{id}/activity?cursor=&limit=
```

**New (this milestone):**
```
GET    /v1/workspaces/{workspace_id}/activity?cursor=&limit=
POST   /v1/workspaces/{workspace_id}/zombies/{zombie_id}:stop
GET    /v1/workspaces/{workspace_id}/zombies/{zombie_id}/billing/summary?period_days=7|30
```

**Reused (existing):**
```
GET    /v1/workspaces/{workspace_id}/billing/summary?period_days=7|30   — workspace-wide total (dashboard)
```

Per-zombie spend is a scope-sliced view of the same data as workspace summary — response schema is identical. No `?group_by=` param; hierarchy lives in the URL per `docs/REST_API_DESIGN_GUIDELINES.md` §2 + §5.

### 9.2 Data sources

All endpoints read from existing tables. No new schema:
- `core.zombies`
- `core.activity_events` (with `idx_activity_events_workspace_created`)
- `zombie_execution_telemetry`
- `core.tenants`

---

## 9.3 Design system + theming (enforced)

All new UI components MUST consume design tokens exclusively. No inline colors, no raw hex, no per-component `dark:` overrides.

**Token stack (already wired in `ui/packages/app/app/globals.css`):**
- Primitives: `@usezombie/design-system/tokens.css` — `--z-bg-*`, `--z-surface-*`, `--z-text-*`, `--z-orange`, `--z-red`, `--z-border`, etc. Same language as `usezombie.com`.
- Semantic bridge (Shadcn-compatible): `--background`, `--foreground`, `--card`, `--popover`, `--primary`, `--secondary`, `--muted`, `--accent`, `--destructive`, `--border`, `--input`, `--ring`, `--radius`.
- Layout tokens: `--sidebar-width`, `--header-height`, `--content-max`.

**Rules:**
1. Components use semantic names only (`bg-background`, `text-foreground`, `border-border`, `bg-destructive`, etc.) via Tailwind utilities or CSS custom props — never the `--z-*` primitives directly. This keeps the primitive layer swappable (future theme variants) without touching components.
2. Dark mode is the token layer's responsibility. Components must look correct in both modes without any `dark:` class; if a component needs a `dark:` override, that is a signal that a token is missing — add the token, don't patch the component.
3. Dialog / Toast / Tooltip / Popover: use the existing Radix + Shadcn primitives in `components/ui/` (already themed). Do not introduce a second dialog or toast system.
4. Motion: Tailwind `transition-*` utilities + CSS `@starting-style` / View Transitions API first. Framer Motion only if a specific interaction genuinely needs it, and gated behind `prefers-reduced-motion`.

**Verify:**
- Code review: grep new .tsx files for hex color literals (`#[0-9a-f]{3,6}`), inline `style={{ color/background }}` with literals, and `dark:` classes. Zero allowed in M12 components.
- Manual: toggle system dark/light; every new page/component renders correctly in both.

---

## 9.4 Shared UI primitives — first-needer-owns policy

Several components will be reused across M11_003, M12, M13, M19, M20, M22. To avoid blocking parallel work, we adopt **option B: first-needer owns each primitive**, placed in `components/ui/` (pure-visual) or `components/domain/` (opinionated shape). Later milestones consume, don't re-implement.

| Primitive | First owner | Location | Notes |
|---|---|---|---|
| `StatusCard` (label + count + variant + optional trend) | M12_001 (this spec, §3) | `components/ui/status-card.tsx` | Used by dashboard, later M20 approvals, M19 lifecycle |
| `Pagination` (cursor + page/limit) | M12_001 (this spec, §4) | `components/ui/pagination.tsx` | Zombies list first; M11_003 admin codes, M20 approvals, M22 grants follow |
| `DataTable` (sortable, row-action, empty state) | M12_001 (this spec, §4) | `components/ui/data-table.tsx` | Single primitive across all list pages |
| `EmptyState` | M12_001 (this spec, §3/§4) | `components/ui/empty-state.tsx` | — |
| `ConfirmDialog` (destructive action) | M12_001 (this spec, §5 kill switch) | `components/ui/confirm-dialog.tsx` | Reused by M13 revoke, M22 revoke, M19 delete |
| `ActivityRow` / `ActivityFeed` | M12_001 (this spec, §3/§5) | `components/domain/activity-feed.tsx` | Workspace-wide + per-zombie variants |
| `SkeletonLoader` primitives | M12_001 (this spec, §3) | `components/ui/skeleton.tsx` | Suspense fallbacks |
| `Banner` (info/success/warn/error, dismissible) | **M11_003** (redemption banner) | `components/ui/banner.tsx` | M12 consumes for degraded-service messaging |
| `CreditsBadge` (header pill, reads credits.remaining) | **M11_003** | `components/domain/credits-badge.tsx` | M12 renders it in Shell header |

**Dimension added for ownership:**
- 9.4.1 PENDING — each primitive listed above owned by M12 must ship with a unit test (`*.test.tsx`) covering default/empty/interactive states — test_type: unit
- 9.4.2 PENDING — each owned primitive must be theme-token-only (zero hex, zero `dark:`) — test_type: code review gate

---

## 10.0 Implementation Constraints

| Constraint | Verify |
|---|---|
| Each page component ≤350 lines (RULE FLL) | `wc -l app/**/*.tsx` |
| Each new Zig handler file ≤350 lines | `wc -l src/http/handlers/*.zig` |
| No credential values in browser state | grep + code review |
| Server components for data fetching; Clerk `auth()` token never crosses to client | code review |
| Clerk auth protects all `(dashboard)` routes | middleware check |
| API errors → toast; no raw error text in UI | component tests |
| All UI uses semantic design tokens only (no hex, no `dark:` in components) | grep new .tsx + manual dark/light toggle |
| Every new `.tsx` in `components/` + `app/(dashboard)/**` has a colocated `*.test.tsx` unit test | `find` sweep at CHORE(close) |
| Every new Zig handler has colocated unit tests (pure logic) + integration tests (HTTP+DB) | tier 1 + tier 2 green |
| Zig: every `conn.query()` has `.drain()` before `deinit()` | `make check-pg-drain` |
| Cross-compile on Linux targets | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| Full lint + memleak + bench before PR | `make lint && make memleak && make bench` |

---

## 11.0 Execution Plan (narrowed Apr 16)

**Precondition:** M24_001 (REST refactor) merged to main — ✅ DONE.

| Step | Action | Verify | Status |
|---|---|---|---|
| 1 | M24_001 merged; branch rebased onto post-refactor main | `git log main..HEAD` clean | ✅ DONE |
| 2 | Backend handlers: workspace activity, zombie :stop, per-zombie billing summary + shared aggregator + upgraded workspace summary | tier-1 unit tests + tier-2 HTTP integration tests green | ✅ DONE (`c6c61e6`, `b6a39ee`) |
| 3 | OpenAPI entries for the 3 new endpoints + `period_days` / `total_cents` on workspace summary | `jq .paths` shows new entries | ✅ DONE (`b6a39ee`) |
| 4 | Shared UI primitives (StatusCard, EmptyState, Pagination, DataTable, ConfirmDialog, ActivityFeed) with 70/70 unit tests | `bun run typecheck / lint / test / build` green | ✅ DONE (`6712f7c`) |
| 5 | Three-layer token pyramid: `globals.css` gains `@theme inline` + `--success`/`--warning`/`--info` + `--primary-bright`/`--primary-glow[-strong]`; `button.tsx` refactored to drop all `var(--z-*)` arbitrary values | grep shows zero `var(--z-` in components | ✅ DONE (`6712f7c`) |
| 6 | `components.json` + `tw-animate-css` + `@custom-variant dark` installed | shadcn CLI contract present | ✅ DONE (`6712f7c`) |
| 7 | CHORE(close): narrow spec, move to done/, write Ripley log, bump VERSION 0.9.0 → 0.18.0, add changelog entry, open PR | spec in `done/`, changelog + log committed | ⏳ IN_PROGRESS |
| 8 | **MOVED_TO_M26:** Design System Unification — self-contained `@usezombie/design-system` package usable by both `ui/packages/app` (Next.js RSC) and `ui/packages/website` (Vite SPA) without framework-specific imports | see `docs/v2/pending/P1_UI_M26_001_DESIGN_SYSTEM_UNIFICATION.md` | — |
| 9 | **MOVED_TO_M27:** typed API client split + 4 pages + firewall placeholder + Shell nav + frontend VERIFY + Vercel preview | see `docs/v2/pending/P1_UI_M27_001_DASHBOARD_PAGES.md` | — |

---

## 12.0 Acceptance Criteria (narrowed Apr 16)

**In-scope for M12_001:**
- [x] M24_001 (REST refactor) merged before M12 EXECUTE starts
- [x] 3 new backend endpoints land with OpenAPI + integration tests (workspace activity, zombie :stop, per-zombie billing summary)
- [x] Workspace billing summary stub upgraded to real data via shared `billing_summary_store` aggregator; no schema change
- [x] Shared UI primitives (StatusCard / EmptyState / Pagination / DataTable / ConfirmDialog / ActivityFeed) shipped with 70/70 colocated unit tests
- [x] Three-layer token pyramid enforced in `globals.css` (`@theme inline` block + extended `:root` + `.dark`)
- [x] `button.tsx` refactored: zero `var(--z-*)` arbitrary values; `data-slot` + `asChild` support preserved
- [x] All Zig handlers ≤ 350 lines; all .tsx files ≤ 350 lines (test files exempt)
- [x] `zig build`, `zig build test`, `make test-integration-db`, `make lint`, `make check-pg-drain`, cross-compile x86_64+aarch64-linux, `bun run typecheck/lint/test/build`, `gitleaks` — all green

**Explicitly deferred (deferred, not dropped):**
- [ ] Dashboard / Zombies / Zombie detail / Firewall / Settings pages → **M27_001**
- [ ] Typed API client split + MSW mocks → **M27_001**
- [ ] Vercel preview deploy + Playwright smoke → **M27_001**
- [ ] `@usezombie/design-system` becomes self-contained + framework-agnostic + used by both app and website → **M26_001**

---

## 13.0 Local Dev — Run the Lead Collector Use Case

The dashboard is wired to `api-dev.usezombie.com` via `ui/packages/app/.env.local` (already set up).

**Seed a demo zombie on dev for visible data:**

```bash
# 1. Create zombie via CLI against dev
ZOMBIE_API_URL=https://api-dev.usezombie.com \
  zombiectl zombie create \
  --workspace $WS_ID \
  --name "Lead Collector (demo)" \
  --description "Seeded for M12 dashboard demo"

# 2. Request an integration grant so activity rows land
zombiectl grant request --zombie $ZOMBIE_ID --service slack \
  --reason "demo grant for dashboard seeding"

# 3. Approve via the Slack DM (or mark approved directly if ops-only)

# 4. Trigger a fake email event to generate activity + telemetry rows
curl -X POST https://api-dev.usezombie.com/v1/webhooks/$ZOMBIE_ID \
  -H "Content-Type: application/json" \
  -d @fixtures/demo_email.json
```

Run the UI:
```bash
cd ui/packages/app
npm run dev        # http://localhost:3000
```

During frontend iteration before backend endpoints land, MSW serves mocked responses matching §9.1.

---

## Applicable Rules

Standard set (`docs/greptile-learnings/RULES.md`). Specific invariants the refactor must honour:
- RULE ORP — cross-layer orphan sweep after the flat → workspace-scoped rename
- RULE FLL — 350-line gate for every touched .zig/.tsx/.ts
- zig-pg-drain — `.drain()` before `deinit()` in every new handler

## Invariants

N/A — no compile-time guardrails.

## Eval Commands

```bash
# E1: Backend
make lint && make test && make test-integration && make check-pg-drain
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux

# E2: Frontend
cd ui/packages/app && npm run lint && npm run test && npm run build

# E3: E2E smoke
cd ui/packages/app && npm run test:e2e:smoke

# E4: Leak + perf
make memleak && make bench
API_BENCH_URL=https://api-dev.usezombie.com/healthz make bench

# E5: 350-line gate
git diff --name-only origin/main \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E6: Gitleaks
gitleaks detect
```

## Dead Code Sweep

After REST refactor: flat-route matcher functions, handler arms, and `zombiectl` helpers that reference flat paths must be removed completely. Enumerate in VERIFY phase, confirm with user before deletion.

## Verification Evidence

| Check | Command | Result | Pass? |
|---|---|---|---|
| Backend tests | `make test && make test-integration` | | |
| pg-drain | `make check-pg-drain` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Frontend tests | `npm run test` | | |
| Frontend build | `npm run build` | | |
| E2E smoke | `npm run test:e2e:smoke` | | |
| Memleak | `make memleak` | | |
| Bench (dev) | `API_BENCH_URL=https://api-dev.usezombie.com/healthz make bench` | | |
| 350L gate | see E5 | | |
| Gitleaks | `gitleaks detect` | | |
| Vercel deploy | `vercel deploy` | | |

---

## Out of Scope (amended)

- **Firewall metrics/events surface** — no backing table or write path; follow-up milestone will extend `outbound_proxy.zig` to persist + add aggregation endpoints.
- **Multi-key API key management** — requires new `api_keys` table with rotation + revocation. Current schema has one hash per tenant; read-only display is all M12 ships.
- **Trust score** — not a real data primitive; drop until defined.
- **Real-time WebSocket updates** — 5s SSE polling only.
- **Chat with running agent** — CLI-only.
- **Dark mode, i18n, mobile-native app.**
- **Zombie configuration editing from UI** — scoped to M19_001.
- **Credentials page** — scoped to M13_001. M12 wires the nav link only.
