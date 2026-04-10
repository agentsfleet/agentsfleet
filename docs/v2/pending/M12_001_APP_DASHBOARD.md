# M12_001: App Dashboard — operator-facing web UI at app.usezombie.com

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 001
**Date:** Apr 10, 2026
**Status:** PENDING
**Priority:** P1 — Operator-facing surface; first web UI for non-CLI users
**Batch:** B5 — after M7 (metrics API), M8 (Slack plugin creates workspaces)
**Branch:** feat/m12-app-dashboard
**Depends on:** M7_001 (firewall metrics API), M4_001 (approval gate), M2_001 (activity stream API)

---

## Overview

**Goal (testable):** `app.usezombie.com` is a web application where operators can: view running Zombie status (active/paused/error), browse the activity stream (last 50 events with live updates), see firewall metrics summary (requests proxied, injections blocked, trust score), track spend (budget used/remaining per Zombie and workspace), and hit the kill switch (stop any Zombie mid-action). The app consumes existing APIs (no new backend logic) and authenticates via Clerk. It is the visual proof that UseZombie works — the thing a CTO opens when they ask "what did the agent do?"

**Problem:** Everything in UseZombie is CLI-first today. The APIs exist (activity stream, firewall metrics, Zombie CRUD, kill switch), but there's no web UI. Engineering managers and CTOs don't use CLIs daily. The surfaces.md defines the app as the "human-facing dashboard for operators" — without it, the product is invisible to the person who approves the purchase. The Slack plugin (M8) gets teams in, but the dashboard is where trust is demonstrated over time.

**Solution summary:** Next.js App Router application at `app.usezombie.com`, deployed on Vercel. Five pages: Dashboard (overview), Workspace (config), Agent Detail (deep view), Firewall (metrics), Credentials (vault management). Clerk for auth (already integrated from v1). All data fetched from existing UseZombie API endpoints via server components. Real-time updates via SSE polling (not WebSocket — simpler, works through CDN). Kill switch is a POST to existing `/v1/workspaces/{ws}/zombies/{id}:stop` endpoint.

---

## 1.0 Dashboard Page (Overview)

**Status:** PENDING

Landing page after login. Shows workspace health at a glance: Zombie status cards (running/paused/error count), recent activity stream (last 50 events), firewall summary (from M7 metrics API), and workspace spend tracker.

**Layout:**
```
┌─────────────────────────────────────────────────┐
│ UseZombie Dashboard                    [ws ▾]   │
├──────────┬──────────┬──────────┬────────────────┤
│ 3 Active │ 1 Paused │ 0 Error  │ $12.40 / $29  │
├──────────┴──────────┴──────────┴────────────────┤
│ Firewall (24h)                                  │
│ 147 proxied · 89 creds injected · 3 blocked     │
│ Trust: 0.97 ↑                                   │
├─────────────────────────────────────────────────┤
│ Recent Activity                                 │
│ 10:47 lead-collector: Email received from j@... │
│ 10:47 lead-collector: Replied with invite code  │
│ 10:45 bug-fixer: PR #247 opened                 │
│ 10:44 bug-fixer: Approval granted (push main)   │
│ ...                                    [See all]│
└─────────────────────────────────────────────────┘
```

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `app/dashboard/page.tsx`
  - input: `Authenticated user with workspace containing 3 active + 1 paused Zombie`
  - expected: `Page renders status cards with correct counts, fetched via GET /v1/workspaces/{ws}/zombies`
  - test_type: integration (API mock)
- 1.2 PENDING
  - target: `app/dashboard/page.tsx`
  - input: `Firewall metrics from GET /v1/workspaces/{ws}/firewall/metrics?period=24h`
  - expected: `Firewall summary bar shows proxied, injected, blocked counts + trust score`
  - test_type: integration (API mock)
- 1.3 PENDING
  - target: `app/dashboard/components/ActivityFeed.tsx`
  - input: `GET /v1/workspaces/{ws}/activity?limit=50`
  - expected: `Activity stream renders with timestamp, zombie name, event description`
  - test_type: unit (component test)
- 1.4 PENDING
  - target: `app/dashboard/components/SpendTracker.tsx`
  - input: `Workspace with $12.40 spent of $29.00 budget`
  - expected: `Progress bar at 42.8%, dollar amounts displayed`
  - test_type: unit (component test)

---

## 2.0 Zombie Detail Page

**Status:** PENDING

Deep view of a single Zombie: full activity log (filterable), firewall events for this Zombie, trust score chart (7-day), spend breakdown, and kill switch button.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `app/zombies/[id]/page.tsx`
  - input: `Zombie ID for active lead-collector`
  - expected: `Page shows name, status, trigger type, skills list, uptime, events processed count`
  - test_type: integration (API mock)
- 2.2 PENDING
  - target: `app/zombies/[id]/components/KillSwitch.tsx`
  - input: `User clicks Kill Switch button for active Zombie`
  - expected: `Confirmation dialog → POST /v1/workspaces/{ws}/zombies/{id}:stop → status updates to "paused"`
  - test_type: integration (API mock)
- 2.3 PENDING
  - target: `app/zombies/[id]/components/ActivityLog.tsx`
  - input: `Zombie with 200 events, filter by "firewall"`
  - expected: `Only firewall events shown, paginated, cursor-based loading`
  - test_type: unit (component test)
- 2.4 PENDING
  - target: `app/zombies/[id]/components/TrustChart.tsx`
  - input: `7 daily trust scores [0.95, 0.97, 0.98, 0.96, 0.99, 0.97, 0.98]`
  - expected: `Line chart with 7 data points, trend indicator (↑/↓/→)`
  - test_type: unit (component test)

---

## 3.0 Firewall Page

**Status:** PENDING

Dedicated firewall metrics view consuming M7's API endpoints. Shows: aggregate metrics (requests proxied, credentials injected, injections blocked, policy violations, trust score), blocked request details (table with time, tool, target, reason), and trust score trend chart.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `app/firewall/page.tsx`
  - input: `GET /v1/workspaces/{ws}/firewall/metrics?period=7d`
  - expected: `Metric cards rendered with correct values, daily trend chart`
  - test_type: integration (API mock)
- 3.2 PENDING
  - target: `app/firewall/components/BlockedTable.tsx`
  - input: `GET /v1/workspaces/{ws}/firewall/events?type=block&limit=20`
  - expected: `Table with columns: time, zombie, tool, target, reason. Paginated.`
  - test_type: unit (component test)
- 3.3 PENDING
  - target: `app/firewall/page.tsx`
  - input: `Period selector changed from 7d to 24h`
  - expected: `Metrics and chart refresh with new period data`
  - test_type: unit (component test)

---

## 4.0 Credentials Page

**Status:** PENDING

Manage the workspace credential vault. List credentials (name, scope, last used — never the value). Add credential (encrypted at rest via API). Delete credential. Usage log (which Zombie, which request, when). This page wraps existing API endpoints.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `app/credentials/page.tsx`
  - input: `GET /v1/workspaces/{ws}/credentials`
  - expected: `Table: name, scope (which Zombies use it), last_used timestamp. No values ever shown.`
  - test_type: integration (API mock)
- 4.2 PENDING
  - target: `app/credentials/components/AddCredentialModal.tsx`
  - input: `User enters name="stripe" and value="sk_test_xxx"`
  - expected: `POST to API, success toast, value field cleared, value NEVER stored in browser state`
  - test_type: unit (component test)
- 4.3 PENDING
  - target: `app/credentials/components/DeleteCredential.tsx`
  - input: `User clicks delete on "stripe" credential`
  - expected: `Confirmation dialog → DELETE to API → credential removed from list`
  - test_type: unit (component test)

---

## 5.0 Settings Page

**Status:** PENDING

Workspace settings: billing plan and usage, API keys (from M9), team members (Clerk-managed). Minimal for v1 — just the essentials.

**Dimensions (test blueprints):**
- 5.1 PENDING
  - target: `app/settings/page.tsx`
  - input: `Workspace on Pro plan, $18 of $29 used`
  - expected: `Plan name, usage bar, upgrade CTA if on Free`
  - test_type: integration (API mock)
- 5.2 PENDING
  - target: `app/settings/components/ApiKeys.tsx`
  - input: `List of 2 API keys`
  - expected: `Table: name, permissions, last_used, created. Revoke button. Create button.`
  - test_type: unit (component test)

---

## 6.0 Auth + Layout

**Status:** PENDING

Clerk auth (existing v1 integration). Workspace selector in nav for users with multiple workspaces. Sidebar navigation: Dashboard, Zombies, Firewall, Credentials, Settings. Responsive layout (desktop primary, mobile passable).

**Dimensions (test blueprints):**
- 6.1 PENDING
  - target: `app/layout.tsx`
  - input: `Unauthenticated user visits app.usezombie.com`
  - expected: `Redirect to Clerk sign-in page`
  - test_type: integration
- 6.2 PENDING
  - target: `app/layout.tsx`
  - input: `Authenticated user with 2 workspaces`
  - expected: `Workspace selector in nav, current workspace highlighted`
  - test_type: unit (component test)
- 6.3 PENDING
  - target: `app/layout.tsx`
  - input: `Mobile viewport (375px)`
  - expected: `Sidebar collapses to hamburger menu, content is usable`
  - test_type: unit (responsive test)

---

## 7.0 Interfaces

**Status:** PENDING

### 7.1 API Endpoints Consumed (all existing)

```
GET  /v1/workspaces/{ws}/zombies                 — list Zombies with status
POST /v1/workspaces/{ws}/zombies/{id}:stop       — kill switch
GET  /v1/workspaces/{ws}/activity?limit=50       — activity stream
GET  /v1/workspaces/{ws}/firewall/metrics        — firewall aggregates
GET  /v1/workspaces/{ws}/firewall/events         — firewall event details
GET  /v1/workspaces/{ws}/credentials             — list credentials
POST /v1/workspaces/{ws}/credentials             — add credential
DELETE /v1/workspaces/{ws}/credentials/{name}    — delete credential
GET  /v1/workspaces/{ws}/api-keys                — list API keys
```

### 7.2 No new backend endpoints

All data comes from existing APIs. The app is a pure frontend consumer.

---

## 8.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Each page component < 400 lines | `wc -l app/**/*.tsx` |
| No credential values stored in browser state (localStorage, sessionStorage, React state) | Code review + grep |
| Server components for data fetching (no client-side API keys in browser) | Code review — all API calls in server components or server actions |
| Clerk auth on all routes | Middleware check |
| All API errors displayed as user-friendly toast messages | Component tests |
| Deployed on Vercel | `vercel deploy` succeeds |
| Responsive: usable at 375px viewport | Visual test |

---

## 9.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Set up Next.js app with Clerk auth + layout | Auth redirect works |
| 2 | Dashboard page (status cards + activity feed + firewall summary) | Tests 1.1-1.4 pass |
| 3 | Zombie detail page (activity log + kill switch + trust chart) | Tests 2.1-2.4 pass |
| 4 | Firewall page (metrics + blocked table) | Tests 3.1-3.3 pass |
| 5 | Credentials page (list + add + delete) | Tests 4.1-4.3 pass |
| 6 | Settings page (billing + API keys) | Tests 5.1-5.2 pass |
| 7 | Responsive layout + mobile test | Test 6.3 pass |
| 8 | Deploy to Vercel | `vercel deploy` succeeds |

---

## 10.0 Acceptance Criteria

**Status:** PENDING

- [ ] Dashboard shows Zombie status, activity, firewall summary, spend — verify: integration test
- [ ] Kill switch stops a Zombie — verify: integration test
- [ ] Firewall page shows metrics and blocked requests — verify: integration test
- [ ] Credentials page never shows credential values — verify: code review + test
- [ ] Clerk auth protects all routes — verify: unauthenticated redirect test
- [ ] Responsive at 375px — verify: viewport test
- [ ] All API errors show user-friendly messages — verify: component tests
- [ ] Deployed on Vercel — verify: `vercel deploy`

---

## 11.0 Out of Scope

- Real-time WebSocket updates (SSE polling at 5s interval for v1)
- Chat with running agent (deferred — CLI-only for now)
- Zombie configuration editing from UI (use CLI or Slack onboarding)
- Dark mode (ship light mode first)
- Internationalization (English only)
- Mobile-native app (responsive web is sufficient)
- Advanced filtering/search on activity stream (basic type filter for v1)
- Custom dashboards or saved views
