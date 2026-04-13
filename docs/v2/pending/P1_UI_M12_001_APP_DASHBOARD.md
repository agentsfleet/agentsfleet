# M12_001: App Dashboard — operator-facing web UI at app.usezombie.com

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 001
**Date:** Apr 10, 2026
**Status:** PENDING
**Priority:** P1 — Operator-facing surface; first web UI for non-CLI users
**Batch:** B5 — after M8 (Slack plugin creates workspaces); M25 (invite signup) ships alongside this batch
**Branch:** feat/m12-app-dashboard
**Depends on:** M4_001 (approval gate), M2_001 (activity stream API)

**Extended by (post-M12 milestones that build on this shell):**
- M13_001 (B5): Credential Vault UI — full vault management; supersedes §4.0 of this spec
- M19_001 (B6): Zombie Lifecycle UI — create, trigger config, firewall rules; supersedes "out of scope: configuration editing"
- M20_001 (B6): Approval Inbox — pending badge on zombie cards, /approvals page, Pending tab on zombie detail
- M21_001 (B6): BYOK Provider — Provider tab on Settings page (§4.0 extended)
- M22_001 (B7): Integration Grants UI — Integrations tab on zombie detail
- M11_003 (B5): Invite Code + Signup — entry point that lands users on this dashboard

---

## Overview

**Goal (testable):** `app.usezombie.com` shows operators the zombie-specific dashboard: running zombie status cards (active/paused/error), activity stream (last 50 events with live updates), firewall metrics summary (requests proxied, injections blocked, trust score), spend tracker (budget used per zombie), and kill switch (stop any zombie mid-action). The app consumes existing APIs and authenticates via Clerk.

**What already exists (`ui/packages/app`):** Next.js App Router shell, Clerk auth (sign-in/sign-up pages), Shell layout with sidebar, workspace list/detail pages, run list/detail pages, API client wired to `api.usezombie.com`, PostHog analytics, and core UI components (badge, button, card, dialog, input). These are not re-built by M12.

**What M12 builds:** The zombie-specific pages and components that do not yet exist — zombie overview, zombie detail, firewall page, settings shell. The existing app is built around v1 workspaces (GitHub repos + spec-to-PR pipeline). M12 adds the v2 zombie surface on top of the same shell without touching the existing workspace/runs pages.

**Solution summary:** New pages layered into the existing `ui/packages/app` App Router: `/zombies` (overview), `/zombies/[id]` (detail), `/firewall` (metrics), `/settings` (shell). Clerk and the Shell layout are inherited as-is. All data from existing UseZombie API endpoints via server components. SSE polling at 5s for live updates. Kill switch posts to `/v1/workspaces/{ws}/zombies/{id}:stop`.

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

## 4.0 Settings Page

**Status:** PENDING

Workspace settings: billing plan and usage, API keys (from M9), team members (Clerk-managed). Minimal for v1 — just the essentials.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `app/settings/page.tsx`
  - input: `Workspace on Pro plan, $18 of $29 used`
  - expected: `Plan name, usage bar, upgrade CTA if on Free`
  - test_type: integration (API mock)
- 4.2 PENDING
  - target: `app/settings/components/ApiKeys.tsx`
  - input: `List of 2 API keys`
  - expected: `Table: name, permissions, last_used, created. Revoke button. Create button.`
  - test_type: unit (component test)

---

## 5.0 Auth + Layout

**Status:** DONE (pre-existing)

Clerk auth (sign-in/sign-up), Shell layout with sidebar, and unauthenticated redirect are already implemented in `ui/packages/app`. M12 does not re-implement these.

M12 adds nav entries to the existing sidebar: Zombies, Firewall, Credentials, Settings. The mobile responsive behaviour of the Shell is inherited as-is.

**Dimension:**
- 5.1 PENDING
  - target: `components/layout/Shell.tsx` — add nav links for /zombies, /firewall, /credentials, /settings
  - input: authenticated user
  - expected: sidebar shows new zombie-surface links alongside existing workspace links
  - test_type: unit (component test)

---

## 6.0 Interfaces

**Status:** PENDING

### 6.1 API Endpoints Consumed (all existing)

```
GET  /v1/workspaces/{ws}/zombies                 — list Zombies with status
POST /v1/workspaces/{ws}/zombies/{id}:stop       — kill switch
GET  /v1/workspaces/{ws}/activity?limit=50       — activity stream
GET  /v1/workspaces/{ws}/firewall/metrics        — firewall aggregates
GET  /v1/workspaces/{ws}/firewall/events         — firewall event details
GET  /v1/workspaces/{ws}/api-keys                — list API keys
```

Credential endpoints (`GET/POST/DELETE /v1/workspaces/{ws}/credentials`) are consumed by M13_001, not M12. M12 provides the nav shell only.

### 6.2 No new backend endpoints

All data comes from existing APIs. The app is a pure frontend consumer.

---

## 7.0 Implementation Constraints (Enforceable)

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

## 8.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Set up Next.js app with Clerk auth + layout | Auth redirect works |
| 2 | Dashboard page (status cards + activity feed + firewall summary) | Tests 1.1-1.4 pass |
| 3 | Zombie detail page (activity log + kill switch + trust chart) | Tests 2.1-2.4 pass |
| 4 | Firewall page (metrics + blocked table) | Tests 3.1-3.3 pass |
| 5 | Settings page (billing + API keys) | Tests 4.1-4.2 pass |
| 6 | Add sidebar nav links (dim 5.1) | zombie-surface links render |
| 7 | Deploy to Vercel | `vercel deploy` succeeds |

Note: credentials page is M13_001. Nav link to `/credentials` is wired in step 1.

---

## 9.0 Acceptance Criteria

**Status:** PENDING

- [ ] Dashboard shows Zombie status, activity, firewall summary, spend — verify: integration test
- [ ] Kill switch stops a Zombie — verify: integration test
- [ ] Firewall page shows metrics and blocked requests — verify: integration test
- [ ] Nav link to /credentials routes to M13_001's page — verify: nav component test
- [ ] Clerk auth protects all routes — verify: unauthenticated redirect test
- [ ] Responsive at 375px — verify: inherited from Shell (pre-existing, no new test dim)
- [ ] All API errors show user-friendly messages — verify: component tests
- [ ] Deployed on Vercel — verify: `vercel deploy`

---

## Applicable Rules

Standard set only.

---

## Invariants

N/A — no compile-time guardrails.

---

## Eval Commands

```bash
# E1: Build
npm run build 2>&1 | head -5; echo "build=$?"

# E2: Tests
npm run test 2>&1 | tail -5; echo "test=$?"

# E3: Lint
npm run lint 2>&1 | grep -E "✓|FAIL"

# E4: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

---

## Dead Code Sweep

N/A — no files deleted.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Build | `npm run build` | | |
| Tests | `npm run test` | | |
| Lint | `npm run lint` | | |
| 350L gate | see E5 | | |
| Gitleaks | `gitleaks detect` | | |
| Vercel deploy | `vercel deploy` | | |

---

## Out of Scope

- Real-time WebSocket updates (SSE polling at 5s interval for v1)
- Chat with running agent (deferred — CLI-only for now)
- Dark mode (ship light mode first)
- Internationalization (English only)
- Mobile-native app (responsive web is sufficient)
- Advanced filtering/search on activity stream (basic type filter for v1)
- Custom dashboards or saved views

Note: "Zombie configuration editing from UI" is NOT out of scope for the product — it is scoped to M19_001 (Zombie Lifecycle UI). M12 does not implement it; M19 does.
