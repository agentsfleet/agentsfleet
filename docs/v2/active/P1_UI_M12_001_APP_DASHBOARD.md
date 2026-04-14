# M12_001: App Dashboard — operator-facing web UI at app.usezombie.com

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 001
**Date:** Apr 10, 2026 (amended Apr 14, 2026)
**Status:** IN_PROGRESS
**Priority:** P1 — Operator-facing surface; first web UI for non-CLI users
**Batch:** B5 — after M8 (Slack plugin creates workspaces); M11_003 (invite signup) ships alongside this batch
**Branch:** feat/m12-app-dashboard
**Worktree:** /Users/kishore/Projects/usezombie-m12-app-dashboard
**Depends on:** M4_001 (approval gate), M2_001 (activity stream API)

**Extended by (post-M12 milestones that build on this shell):**
- M13_001 (B5): Credential Vault UI — full vault management; supersedes §4.0 of this spec
- M19_001 (B6): Zombie Lifecycle UI — create, trigger config, firewall rules; supersedes "out of scope: configuration editing"
- M20_001 (B6): Approval Inbox — pending badge on zombie cards, /approvals page, Pending tab on zombie detail
- M21_001 (B6): BYOK Provider — Provider tab on Settings page (§4.0 extended)
- M22_001 (B7): Integration Grants UI — Integrations tab on zombie detail
- M11_003 (B5): Invite Code + Signup — entry point that lands users on this dashboard

---

## Amendment note (Apr 14, 2026)

The original spec (Apr 10) assumed workspace-scoped REST endpoints (`/v1/workspaces/{ws}/zombies/...`) already existed. A data/route audit on `main` (commit `a85ae78`) found the API was flat (`/v1/zombies/?workspace_id=`) and several consumed endpoints (kill switch, firewall metrics, api-keys) did not exist at all. This amendment:

1. **Adds a REST URL refactor as the first workstream** — move flat `/v1/zombies/*` to workspace-scoped `/v1/workspaces/{ws}/zombies/*`. Query params reserved for `page`, `limit`, `cursor`, `search`.
2. **Adds Tier A backend endpoints** that are cheap to build on existing schema (workspace activity, kill switch, per-zombie spend aggregation).
3. **Defers to follow-up milestones**: full firewall page (no backing data today), multi-key API key management (schema has single `api_key_hash` per tenant), trust score (not a real primitive yet).
4. **UI is built against mocked responses first, swapped to real endpoints as they land.** Lets frontend and backend iterate in parallel.

---

## Overview

**Goal (testable):** `app.usezombie.com` shows operators the zombie-specific dashboard for the 3 canonical zombie use cases (Lead Collector, Hiring Agent, Ops Zombie — see `docs/nostromo/lead_collector_zombie.md`): running zombie status cards, workspace-wide activity stream, per-zombie spend from execution telemetry, kill switch, and navigation entries for future milestones' pages. All paths are REST-ful and workspace-scoped. The app consumes existing + a handful of new endpoints and authenticates via Clerk.

**Existing (`ui/packages/app`):** Next.js 16 App Router, Clerk auth, Shell layout with sidebar, workspace list/detail (v1), API client (`lib/api.ts`) with Bearer token pattern, Vitest + Playwright, PostHog.

**M12 adds:** REST URL refactor on the API side + 4 new frontend pages (+ 1 placeholder) layered into the existing `(dashboard)` route group.

**Solution summary:**
- **API refactor (workstream 1):** flat `/v1/zombies/*` → workspace-scoped `/v1/workspaces/{ws}/zombies/*`. Update handlers, router, OpenAPI, `zombiectl`, tests.
- **Backend adds (workstream 2):** `GET /v1/workspaces/{ws}/activity`, `POST /v1/workspaces/{ws}/zombies/{id}:stop`, `GET /v1/workspaces/{ws}/zombies/{id}/spend`.
- **Frontend (workstream 3):** `/dashboard` overview, `/zombies` list, `/zombies/[id]` detail, `/settings` (minimal), `/firewall` placeholder. Sidebar nav links.

---

## 1.0 REST URL Refactor (workstream 1)

**Status:** PENDING

Flat routes → workspace-scoped paths. Query params reserved for pagination and search.

**Route migration table:**

| From | To |
|---|---|
| `GET /v1/zombies/?workspace_id=X` | `GET /v1/workspaces/{ws}/zombies?page=&limit=&search=` |
| `POST /v1/zombies/` (body: workspace_id) | `POST /v1/workspaces/{ws}/zombies` |
| `DELETE /v1/zombies/{id}` | `DELETE /v1/workspaces/{ws}/zombies/{id}` |
| `GET /v1/zombies/activity?zombie_id=X` | `GET /v1/workspaces/{ws}/zombies/{id}/activity?cursor=&limit=` |
| `POST /v1/zombies/{id}/integration-requests` | `POST /v1/workspaces/{ws}/zombies/{id}/integration-requests` |
| `GET /v1/zombies/{id}/integration-grants` | `GET /v1/workspaces/{ws}/zombies/{id}/integration-grants` |
| `DELETE /v1/zombies/{id}/integration-grants/{gid}` | `DELETE /v1/workspaces/{ws}/zombies/{id}/integration-grants/{gid}` |

**Rule:** pre-v2.0 (VERSION=`0.9.0`), no HTTP 410 stubs for the removed flat paths — per project feedback (`feedback_pre_v2_api_drift.md`), bare 404s are acceptable in teardown era.

**Dimensions:**
- 1.1 PENDING — target `src/http/router.zig` — input: request paths from migration table — expected: workspace-scoped paths resolve; flat paths 404 — test_type: integration
- 1.2 PENDING — target `src/http/route_matchers.zig` — input: new path matchers for `/v1/workspaces/{ws}/zombies/...` — expected: workspace_id + zombie_id extracted correctly; malformed rejected — test_type: unit
- 1.3 PENDING — target handlers in `src/http/handlers/zombie_api.zig`, `zombie_activity_api.zig`, `integration_grants.zig` — input: new paths with path-param workspace_id — expected: `workspace_id` from path, not query — test_type: integration
- 1.4 PENDING — target `zombiectl` (npm package) — input: existing subcommands that call flat routes — expected: all updated to call workspace-scoped paths — test_type: unit (mocked HTTP) + e2e smoke
- 1.5 PENDING — target `openapi.json` — input: regenerated from new routes — expected: paths use `{workspaceId}` path params, query params limited to paging/search — test_type: schema diff

---

## 2.0 Backend Endpoints Added (workstream 2)

**Status:** PENDING

Three new endpoints backed by existing tables. Each ≤100 lines.

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

- 2.1 PENDING — target `src/http/handlers/workspace_activity.zig` — input: workspace with 3 zombies each with 20 events — expected: merged feed newest-first, cursor pagination — test_type: integration

### 2.2 Kill switch

`POST /v1/workspaces/{ws}/zombies/{id}:stop`

Flips `core.zombies.status` from `active`/`paused` to `stopped`. `stopped` is terminal per the schema comment. Returns 409 if already terminal.

- 2.2 PENDING — target `src/http/handlers/zombie_lifecycle.zig` (new) — input: active zombie — expected: status=stopped after call, activity event recorded, 409 on re-call — test_type: integration
- 2.3 PENDING — same — input: zombie not in workspace — expected: 404 — test_type: integration

### 2.3 Per-zombie spend summary

`GET /v1/workspaces/{ws}/zombies/{id}/spend?period=7d|30d`

Aggregates `zombie_execution_telemetry.credit_deducted_cents` grouped by zombie, windowed by `recorded_at`.

Response:
```json
{
  "zombie_id": "...", "period_days": 7,
  "total_cents": 1240, "event_count": 47,
  "first_event_at": 1744..., "last_event_at": 1744...
}
```

- 2.4 PENDING — target `src/http/handlers/zombie_spend.zig` (new) — input: zombie with 47 telemetry rows totalling 1240 cents in 7d — expected: response matches — test_type: integration
- 2.5 PENDING — same — input: zombie with no telemetry — expected: zeros, not 404 — test_type: integration

---

## 3.0 Dashboard Page (Overview)

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
- 3.3 PENDING — target `components/domain/SpendTracker.tsx` — input: per-zombie `/spend?period=7d` responses summed — expected: total cents displayed, per-zombie breakdown on hover — test_type: unit

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
- 5.4 PENDING — target `app/(dashboard)/zombies/[id]/components/SpendPanel.tsx` — input: 7d + 30d spend calls — expected: both values shown, event count — test_type: unit

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
GET    /v1/workspaces/{ws}/activity?cursor=&limit=
POST   /v1/workspaces/{ws}/zombies/{id}:stop
GET    /v1/workspaces/{ws}/zombies/{id}/spend?period=7d|30d
```

### 9.2 Data sources

All endpoints read from existing tables. No new schema:
- `core.zombies`
- `core.activity_events` (with `idx_activity_events_workspace_created`)
- `zombie_execution_telemetry`
- `core.tenants`

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
| Zig: every `conn.query()` has `.drain()` before `deinit()` | `make check-pg-drain` |
| Cross-compile on Linux targets | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| Full lint + memleak + bench before PR | `make lint && make memleak && make bench` |

---

## 11.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | REST refactor: router + route_matchers + handlers for existing flat routes | Tests 1.1-1.3 pass; existing integration tests updated and green |
| 2 | `zombiectl` updated to new paths | Tier 1 `make test` in npm package; smoke against `api-dev` |
| 3 | OpenAPI regen | `openapi.json` diff reviewed |
| 4 | New backend: workspace activity, kill switch, per-zombie spend | Tests 2.1-2.5 pass (tier 1 + 2) |
| 5 | Frontend: extend `lib/api.ts` against MSW mocks | `npm run test` green |
| 6 | Dashboard page + ActivityFeed + SpendTracker | Tests 3.1-3.3 pass |
| 7 | Zombies list page | Tests 4.1-4.2 pass |
| 8 | Zombie detail + ActivityLog + SpendPanel + KillSwitch | Tests 5.1-5.4 pass |
| 9 | Firewall placeholder + Settings + Shell nav | Tests 6.1, 7.1, 8.1 pass |
| 10 | Swap MSW for real `api-dev`, manual smoke | Pages render with real data |
| 11 | Tier 3 verification: `make down && make up && make test-integration` | Green |
| 12 | Vercel preview deploy | Deploy succeeds, smoke against preview URL |

---

## 12.0 Acceptance Criteria

- [ ] All flat `/v1/zombies/*` routes removed; workspace-scoped paths serve equivalent behaviour
- [ ] `zombiectl` works against refactored API — existing CLI smoke tests green
- [ ] OpenAPI paths updated; no query param carries identity data
- [ ] Dashboard shows zombie status counts, activity feed, spend summary against `api-dev`
- [ ] Zombies list paginates and searches
- [ ] Zombie detail shows activity log, 7d + 30d spend, working kill switch
- [ ] Firewall page renders placeholder (no API calls)
- [ ] Settings shows workspace info + masked tenant key
- [ ] Clerk auth protects all `(dashboard)` routes
- [ ] All Zig handlers ≤350 lines; all .tsx pages ≤350 lines
- [ ] `make lint && make test && make test-integration && make memleak && make bench` all green
- [ ] Vercel preview deployed and manually smoked

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
