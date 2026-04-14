# M22_001: Integration Grants UI — View, Request, and Revoke Grants per Zombie

**Prototype:** v2
**Milestone:** M22
**Workstream:** 001
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P1 — UZ-GRANT-001 is the most common failure mode; dashboard has no grants surface
**Batch:** B7 — after M19 (zombie lifecycle UI provides the detail page shell)
**Branch:** feat/m22-grants-ui
**Depends on:** M12_001 (app shell), M19_001 (zombie detail page), M9_001 (grants API, done)

---

## Overview

**Goal (testable):** An operator whose zombie is failing with `UZ-GRANT-001` can open the dashboard, navigate to the zombie's detail page, see that the HubSpot grant is "Pending" (not yet approved), click "Request approval", see the Slack DM land within 5 seconds, approve it, and watch the grant status flip to "Approved" — all without opening a terminal. An operator who wants to revoke a grant sees which pending actions will be blocked and confirms deletion.

**Problem:** Integration grants are the mechanism by which an operator trusts a zombie to call a specific service. The setup docs for every zombie archetype (Lead Collector, Hiring Agent, Ops Zombie) list grant setup as a required step, and list `UZ-GRANT-001` / `UZ-GRANT-002` as the top troubleshooting items. Currently, grants are entirely CLI-managed: `zombiectl grant request`, `zombiectl grant list`. The dashboard has no grants panel. An operator who sees `UZ-GRANT-001` in the activity stream cannot act on it from the dashboard — they must open a terminal, run `zombiectl grant request`, wait for a Slack DM, and approve it there. For non-CLI operators this is a hard stop.

**Solution summary:** Add a "Integrations" section to the zombie detail page showing all grants with status, last-used timestamp, and a Revoke button. Add a "Request Grant" flow for adding new service access. Add a grant health indicator dot on zombie cards in the dashboard overview (green = all approved, yellow = some pending, red = any revoked). Uses the existing M9_001 grants API.

**DX paths:**

| Action | CLI | UI (this milestone) | API |
|---|---|---|---|
| List grants | `zombiectl grant list --zombie {id}` | Integrations panel | `GET /v1/workspaces/{ws}/zombies/{id}/grants` |
| Request grant | `zombiectl grant request --zombie {id} --service slack` | Request Grant modal | `POST /v1/workspaces/{ws}/zombies/{id}/integration-requests` |
| Revoke grant | `zombiectl grant revoke --zombie {id} --service slack` | Revoke button + confirm | `DELETE /v1/workspaces/{ws}/grants/{grant_id}` |

---

## 1.0 Grants Panel on Zombie Detail Page

**Status:** PENDING

A new "Integrations" tab on the zombie detail page. Lists all services the zombie has requested access to, regardless of status. Each row shows service name, status, approved_at or pending_since, last_used, and actions (Revoke for approved; Cancel for pending).

**Layout:**

```
Zombie detail: lead-collector

[Activity] [Pending (2)] [Integrations] [Memory] [Config]

Integrations
┌──────────────────────────────────────────────────────────────────────┐
│ SERVICE     STATUS      APPROVED          LAST USED    ACTIONS       │
├──────────────────────────────────────────────────────────────────────┤
│ slack       ✓ Approved  Apr 12, 10:31     10 min ago   [Revoke]      │
│ hubspot     ✓ Approved  Apr 12, 10:32     5 min ago    [Revoke]      │
│ agentmail   ⏳ Pending   Requested Apr 13  —            [Cancel]      │
└──────────────────────────────────────────────────────────────────────┘

[+ Request new integration]
```

**Dimensions:**
- 1.1 PENDING
  - target: `app/zombies/[id]/components/GrantsPanel.tsx`
  - input: zombie with 2 approved grants (slack, hubspot) and 1 pending (agentmail)
  - expected: table renders all 3 with correct status badges, approved_at, last_used columns
  - test_type: unit (component test)
- 1.2 PENDING
  - target: `app/zombies/[id]/components/GrantsPanel.tsx`
  - input: zombie with 0 grants
  - expected: empty state: "No integrations yet. Request access to services this zombie will call." + Request button
  - test_type: unit (component test)
- 1.3 PENDING
  - target: `app/zombies/[id]/components/GrantsPanel.tsx`
  - input: zombie with a revoked grant
  - expected: status shows "✗ Revoked" in red; row highlighted; `UZ-GRANT-003` tooltip on hover
  - test_type: unit (component test)

---

## 2.0 Request Grant Flow

**Status:** PENDING

An operator who needs to add a new service integration clicks "Request new integration". Modal: service selector (common services pre-listed with logos), reason field (pre-filled from archetype template if zombie was created via wizard), and submit. On submit: `POST /v1/workspaces/{ws}/zombies/{id}/integration-requests` → Slack DM lands within seconds → status shows "Pending".

**Layout:**

```
Request Integration Grant

Service      [Slack ▾]
             ● Slack  ● HubSpot  ● Grafana  ● Lever  ● Custom

Reason       [Need to post lead notifications to #leads]

Approval via  Slack DM to workspace owner

             [Cancel]    [Request Access →]
```

**Dimensions:**
- 2.1 PENDING
  - target: `app/zombies/[id]/components/RequestGrantModal.tsx`
  - input: user selects "HubSpot" and enters reason, clicks Request Access
  - expected: `POST /v1/workspaces/{ws}/zombies/{id}/integration-requests` with `{ service: "hubspot", reason: "..." }` — modal closes, grant appears as "Pending" in table
  - test_type: integration (API mock)
- 2.2 PENDING
  - target: `app/zombies/[id]/components/RequestGrantModal.tsx`
  - input: user requests grant for service already approved
  - expected: inline error "slack integration is already approved" — submission blocked
  - test_type: unit (component test)
- 2.3 PENDING
  - target: `app/zombies/[id]/components/RequestGrantModal.tsx`
  - input: empty reason field
  - expected: client-side validation "Reason is required — explain what this zombie will use this service for"
  - test_type: unit (component test)

---

## 3.0 Revoke Grant Flow

**Status:** PENDING

Revocation shows impact before confirming: which pending gate actions reference this service and will be blocked. For an approved grant with no pending gates, the dialog is simple. For a grant with pending gates, it warns.

**Layout:**

```
Revoke HubSpot integration?

This zombie currently has 1 pending action that uses HubSpot:
  • POST api.hubspot.com — waiting for approval (23 min)

Revoking will immediately block these actions. The zombie will
receive UZ-GRANT-003 on the next HubSpot call.

[Cancel]    [Revoke anyway →]
```

**Dimensions:**
- 3.1 PENDING
  - target: `app/zombies/[id]/components/RevokeGrantDialog.tsx`
  - input: revoke "hubspot" grant, zombie has 1 pending HubSpot gate action
  - expected: dialog shows warning with the pending action listed; Revoke anyway button
  - test_type: unit (component test)
- 3.2 PENDING
  - target: `app/zombies/[id]/components/RevokeGrantDialog.tsx`
  - input: revoke "slack" grant, zombie has 0 pending slack actions
  - expected: simple confirmation: "No pending actions use Slack. Safe to revoke." + Revoke button
  - test_type: unit (component test)
- 3.3 PENDING
  - target: `app/zombies/[id]/components/RevokeGrantDialog.tsx`
  - input: user confirms revocation
  - expected: `DELETE /v1/workspaces/{ws}/grants/{grant_id}` → grant row updates to "✗ Revoked"
  - test_type: integration (API mock)

---

## 4.0 Grant Health Indicator on Dashboard Cards

**Status:** PENDING

Each zombie card in the dashboard overview shows a small grant health dot. Green = all grants approved. Yellow = at least one grant pending. Red = at least one grant revoked (zombie will fail on next call to that service).

**Dimensions:**
- 4.1 PENDING
  - target: `app/dashboard/components/ZombieCard.tsx`
  - input: zombie with 2 approved grants + 1 pending grant
  - expected: yellow dot with tooltip "1 integration pending approval"
  - test_type: unit (component test)
- 4.2 PENDING
  - target: `app/dashboard/components/ZombieCard.tsx`
  - input: zombie with 1 revoked grant
  - expected: red dot with tooltip "1 integration revoked — zombie will fail on {service} calls"
  - test_type: unit (component test)
- 4.3 PENDING
  - target: `app/dashboard/components/ZombieCard.tsx`
  - input: zombie with all grants approved
  - expected: green dot
  - test_type: unit (component test)

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 API Endpoints (existing from M9_001)

```
GET    /v1/workspaces/{ws}/zombies/{id}/grants          — list grants with status
POST   /v1/workspaces/{ws}/zombies/{id}/integration-requests — request new grant
DELETE /v1/workspaces/{ws}/grants/{grant_id}            — revoke grant
```

### 5.2 No new backend endpoints

All operations use the M9_001 grant API. This is a pure frontend milestone.

### 5.3 Error Contracts

| Error condition | Code | HTTP |
|---|---|---|
| Grant not found | `UZ-GRANT-004` | 404 |
| Grant already approved (duplicate request) | inline validation — blocked client-side | — |
| Revoke while zombie running | `UZ-GRANT-005` | 409 — warn, don't block |

---

## 6.0 Implementation Constraints

| Constraint | How to verify |
|---|---|
| Each component < 400 lines | `wc -l` |
| Grant health computed from grants list (no extra API call) | Code review |
| Revoke dialog always shows pending action count | Dim 3.1 + 3.2 |

---

## 7.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | Grants panel (Integrations tab) | Dims 1.1–1.3 |
| 2 | Request Grant modal | Dims 2.1–2.3 |
| 3 | Revoke Grant dialog with impact | Dims 3.1–3.3 |
| 4 | Grant health dot on dashboard cards | Dims 4.1–4.3 |
| 5 | Full test + lint | all dims pass |

---

## 8.0 Acceptance Criteria

- [ ] Grants panel shows all grants with status — verify: dim 1.1
- [ ] Request grant → Pending in table — verify: dim 2.1
- [ ] Revoke with impact analysis — verify: dim 3.1
- [ ] Grant health dot on dashboard cards — verify: dims 4.1–4.3

---

## Applicable Rules

Standard Next.js set. RULE FLL (350-line gate).

---

## Eval Commands

```bash
npm run build 2>&1 | head -5; echo "build=$?"
npm run test 2>&1 | tail -5
npm run lint 2>&1 | grep -E "✓|FAIL"
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

---

## Out of Scope

- Grant scopes narrowing (for V1, grants are full-access `*`; scoped grants are future)
- Grant transfer between workspaces
- Automated grant re-request on UZ-GRANT-001 (would require retry policy — future)
