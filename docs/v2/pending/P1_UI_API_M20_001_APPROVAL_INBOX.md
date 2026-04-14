# M20_001: Approval Inbox — Pending Gate Actions Surfaced in the Dashboard

**Prototype:** v2
**Milestone:** M20
**Workstream:** 001
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P1 — Without this, the dashboard gives false confidence; stalled zombies appear "Active"
**Batch:** B6 — after M12 (app shell); M4_001 (approval gate) already done
**Branch:** feat/m20-approval-inbox
**Depends on:** M12_001 (app shell), M4_001 (approval gate, done)

---

## Overview

**Goal (testable):** An operator opening the dashboard who has a zombie stalled at an approval gate sees: (1) a badge on the zombie's status card showing "N pending", (2) a dedicated `/approvals` page listing all pending gate actions across the workspace with full action details, (3) Approve and Deny buttons that resolve the gate immediately from the browser, (4) the zombie resuming (or receiving denied) within 2 seconds of the click. This must also be accessible via the API for programmatic orchestrators that auto-approve routine actions.

**Problem:** All approval gate interactions currently flow through Slack DMs only (from M4_001's implementation). If a zombie hits a gate and the operator is looking at the dashboard, they see the zombie as "Active" — no indication it's been waiting. For the Hiring Agent, an offer letter pending approval is invisible in the dashboard. For the Blog Writer, a post ready to publish is invisible. For the Ops Zombie, an on-call page waiting for sign-off is invisible. The dashboard actively misleads — "Active" reads as healthy, not stalled.

Slack DMs are not being removed (multi-channel deduplication already works). This milestone adds the dashboard as a parallel approval channel. Clicking Approve in the browser resolves the gate; Slack's buttons become stale no-ops.

**Solution summary:** New API endpoints over the existing M4_001 gate state: `GET /v1/workspaces/{ws}/approvals` (list pending), `POST /v1/workspaces/{ws}/approvals/{gate_id}:approve`, `POST /v1/workspaces/{ws}/approvals/{gate_id}:deny`. Dashboard surfaces: (a) badge on zombie cards in the dashboard overview, (b) `/approvals` page with a table of pending actions and an inline detail/approve flow, (c) "Pending" tab on the zombie detail page.

**DX paths:**

| Action | Current path | This milestone |
|---|---|---|
| See pending approvals | Slack DM arrives | Dashboard badge + /approvals page |
| Approve gate | Click in Slack | Click in dashboard OR `POST /v1/approvals/{id}:approve` |
| Deny gate | Click in Slack | Click in dashboard OR `POST /v1/approvals/{id}:deny` |
| Auto-approve (pipeline) | Not possible | API `POST :approve` with workspace token |

---

## 1.0 Approval Badge on Dashboard Cards

**Status:** PENDING

Zombie status cards on the dashboard overview show a count badge when that zombie has pending gate actions. Badge color: orange. Clicking the badge navigates to the zombie's detail page → Pending tab.

**Layout:**

```
┌──────────┬──────────┬──────────┬────────────────┐
│ 3 Active │ 1 Paused │ 0 Error  │ $12.40 / $29   │
└──────────┴──────────┴──────────┴────────────────┘

Zombie cards:
┌────────────────────────────────────┐
│ lead-collector          ● Active   │
│                        [2 pending] │  ← orange badge
└────────────────────────────────────┘
┌────────────────────────────────────┐
│ blog-writer             ● Active   │
│                        [1 pending] │
└────────────────────────────────────┘
```

**Dimensions:**
- 1.1 PENDING
  - target: `app/dashboard/components/ZombieCard.tsx`
  - input: zombie with 2 pending gate actions
  - expected: orange badge showing "2 pending"; clicking navigates to zombie detail → Pending tab
  - test_type: unit (component test)
- 1.2 PENDING
  - target: `app/dashboard/components/ZombieCard.tsx`
  - input: zombie with 0 pending gate actions
  - expected: no badge rendered
  - test_type: unit (component test)

---

## 2.0 Global Approvals Page (/approvals)

**Status:** PENDING

Workspace-wide view. All pending gates across all zombies, sorted oldest-first (most urgent). Each row is expandable to show full action details.

**Layout:**

```
/approvals

Pending Approvals (3)
┌───────────────────────────────────────────────────────────────────────┐
│ ZOMBIE          ACTION           TOOL        WAITING    DETAILS       │
├───────────────────────────────────────────────────────────────────────┤
│ hiring-agent    create_offer     lever_api   47 min     [▶ Expand]    │
│ blog-writer     publish_post     ghost_api   23 min     [▶ Expand]    │
│ ops-zombie      page_oncall      slack       4 min      [▶ Expand]    │
└───────────────────────────────────────────────────────────────────────┘

Expanded row (hiring-agent / create_offer):
┌────────────────────────────────────────────────────┐
│ Tool: lever_api                                    │
│ Action: create_offer                               │
│ Target: hire.lever.co/v1/offers                    │
│ Details:                                           │
│   Candidate: Jane Smith                            │
│   Role: Senior Engineer                            │
│   Salary: $180,000                                 │
│   Start date: May 1                                │
│                                                    │
│ Waiting since: 10:12 AM (47 min ago)               │
│                                                    │
│          [Deny]           [Approve →]              │
└────────────────────────────────────────────────────┘
```

**Dimensions:**
- 2.1 PENDING
  - target: `app/approvals/page.tsx`
  - input: workspace with 3 pending gates across 3 zombies
  - expected: table renders all 3, sorted by oldest first, shows zombie name + action + tool + wait time
  - test_type: integration (API mock)
- 2.2 PENDING
  - target: `app/approvals/page.tsx`
  - input: user expands a row
  - expected: full action details shown: target URL, body fields, waiting since timestamp
  - test_type: unit (component test)
- 2.3 PENDING
  - target: `app/approvals/page.tsx`
  - input: user clicks Approve on expanded row
  - expected: `POST /v1/workspaces/{ws}/approvals/{gate_id}:approve` → row removed from list → success toast
  - test_type: integration (API mock)
- 2.4 PENDING
  - target: `app/approvals/page.tsx`
  - input: user clicks Deny on expanded row
  - expected: optional reason textarea → `POST /v1/workspaces/{ws}/approvals/{gate_id}:deny` → row removed
  - test_type: integration (API mock)
- 2.5 PENDING
  - target: `app/approvals/page.tsx`
  - input: empty workspace — no pending gates
  - expected: empty state: "No pending approvals. Zombies are running autonomously."
  - test_type: unit (component test)

---

## 3.0 Pending Tab on Zombie Detail Page

**Status:** PENDING

Per-zombie view. Tabs: Activity (M12 §2.3), Pending, History. Pending tab shows gates waiting for this zombie. History tab shows resolved approvals (approved/denied, by whom, when).

**Dimensions:**
- 3.1 PENDING
  - target: `app/zombies/[id]/page.tsx`
  - input: zombie with 1 pending gate
  - expected: Pending tab badge shows "(1)"; tab content shows the gated action detail
  - test_type: unit (component test)
- 3.2 PENDING
  - target: `app/zombies/[id]/components/ApprovalHistory.tsx`
  - input: zombie with 5 resolved approvals (mix of approved + denied)
  - expected: History tab shows timestamp, action, resolver (Slack / Dashboard / API), outcome
  - test_type: unit (component test)
- 3.3 PENDING
  - target: real-time update
  - input: zombie hits a gate while operator has the detail page open (SSE polling at 5s)
  - expected: Pending badge appears without page reload within 5 seconds
  - test_type: integration

---

## 4.0 API Path (Programmatic Approval)

**Status:** PENDING

A CI/CD pipeline or orchestrator can approve gates programmatically. Use case: an automated deployment pipeline that approves its own "push to production" gate based on build health, without a human in the loop.

```bash
# List pending approvals
curl https://api.usezombie.com/v1/workspaces/{ws}/approvals?status=pending \
  -H "Authorization: Bearer wt_..."

# Approve a gate
curl -X POST https://api.usezombie.com/v1/workspaces/{ws}/approvals/gate_01abc:approve \
  -H "Authorization: Bearer wt_..."
  -d '{ "reason": "build health: green, p99 latency: 48ms" }'

# Deny a gate
curl -X POST https://api.usezombie.com/v1/workspaces/{ws}/approvals/gate_01abc:deny \
  -H "Authorization: Bearer wt_..."
  -d '{ "reason": "p99 latency spike: 340ms — holding deploy" }'
```

**Dimensions:**
- 4.1 PENDING
  - target: `GET /v1/workspaces/{ws}/approvals?status=pending`
  - input: workspace token, 3 pending gates
  - expected: `{ gates: [{ gate_id, zombie_id, zombie_name, tool, target, body_summary, created_at }], total: 3 }`
  - test_type: integration
- 4.2 PENDING
  - target: `POST /v1/workspaces/{ws}/approvals/{gate_id}:approve`
  - input: workspace token + gate_id
  - expected: gate resolved; zombie execution resumes; `{ outcome: "approved", resolved_at, resolved_by: "api" }`
  - test_type: integration
- 4.3 PENDING
  - target: `POST /v1/workspaces/{ws}/approvals/{gate_id}:deny`
  - input: workspace token + gate_id + optional reason
  - expected: gate resolved; zombie receives `gate_denied` error; reason stored in history
  - test_type: integration
- 4.4 PENDING
  - target: double-resolution prevention
  - input: approve a gate that was already resolved by Slack click
  - expected: 409 `UZ-GATE-002 Gate already resolved`
  - test_type: integration

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 New API Endpoints (over existing M4_001 gate state)

```
GET  /v1/workspaces/{ws}/approvals                    — list all gates (filter: ?status=pending|resolved&zombie_id=)
GET  /v1/workspaces/{ws}/approvals/{gate_id}          — get single gate with full body details
POST /v1/workspaces/{ws}/approvals/{gate_id}:approve  — approve gate (optional reason)
POST /v1/workspaces/{ws}/approvals/{gate_id}:deny     — deny gate (optional reason)
```

### 5.2 M4_001 Gate State (existing, no changes)

The gate is still created by M4_001's firewall intercept. The gate's Slack DM still fires. M20 adds a parallel read/resolve surface. Gate state stores: `gate_id`, `zombie_id`, `workspace_id`, `tool`, `target`, `method`, `body_summary`, `created_at`, `resolved_at`, `resolved_by` (slack|dashboard|api), `outcome` (approved|denied), `reason`.

### 5.3 Error Contracts

| Error condition | Code | HTTP |
|---|---|---|
| Gate not found | `UZ-GATE-001` | 404 |
| Gate already resolved | `UZ-GATE-002` | 409 |
| Gate expired (default 2h TTL) | `UZ-GATE-003` | 410 |

---

## 6.0 Implementation Constraints (Enforceable)

| Constraint | How to verify |
|---|---|
| Dashboard Approve/Deny and Slack Approve/Deny both work; second click is a no-op | Dim 4.4 |
| Action body fields shown in preview never contain credential values | grep + code review |
| SSE polling picks up new gates within 5s | Dim 3.3 |
| Each component < 400 lines | `wc -l` |

---

## 7.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | New Zig handlers: `GET /v1/workspaces/{ws}/approvals` + single gate GET | dims 4.1 |
| 2 | `POST :approve` and `POST :deny` handlers + idempotency check | dims 4.2–4.4 |
| 3 | Approval badge on zombie cards | dims 1.1–1.2 |
| 4 | Global `/approvals` page | dims 2.1–2.5 |
| 5 | Pending tab + History tab on zombie detail | dims 3.1–3.3 |
| 6 | Cross-compile (Zig) + full test gate | all dims pass |

---

## 8.0 Acceptance Criteria

- [ ] Zombie with pending gate shows orange badge on dashboard — verify: dim 1.1
- [ ] Approve from dashboard resumes zombie — verify: dim 2.3 + 4.2
- [ ] Deny from dashboard sends zombie gate_denied — verify: dim 2.4 + 4.3
- [ ] Slack Approve after dashboard Approve → 409 no-op — verify: dim 4.4
- [ ] API programmatic approve works — verify: dim 4.2
- [ ] New gate appears in dashboard within 5s (SSE) — verify: dim 3.3
- [ ] Empty state when no pending approvals — verify: dim 2.5

---

## Applicable Rules

RULE FLL, RULE FLS (drain — Zig handlers), RULE XCC (cross-compile), RULE EP4 (no endpoints removed).

---

## Eval Commands

```bash
# E1: Zig build
zig build 2>&1 | head -5; echo "zig_build=$?"

# E2: Tests
make test 2>&1 | tail -5
make test-integration 2>&1 | grep -i "approval\|gate" | tail -10

# E3: Next.js build
npm run build 2>&1 | head -5

# E4: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E5: pg-drain
make check-pg-drain 2>&1 | tail -3

# E6: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

---

## Out of Scope

- Push notifications (browser push API) for new pending approvals — SSE polling at 5s is sufficient for V1
- Approval delegation (assign another team member to approve) — depends on team permissions model (future)
- Bulk approve all pending — too risky for V1; each gate requires individual review
- Mobile push notifications
