# M19_001: Zombie Lifecycle UI — Install, Configure, and Manage Zombies from the Dashboard

**Prototype:** v2
**Milestone:** M19
**Workstream:** 001
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P1 — Without this, the dashboard is read-only; operators must use the CLI for all setup
**Batch:** B2 — alpha gate, parallel with M11_005, M13_001, M21_001, M27_001, M31_001, M33_001
**Branch:** feat/m19-zombie-lifecycle-ui
**Depends on:** M12_001 (app shell + layout), M9_001 (grants/execute API, done)

---

## Overview

**Goal (testable):** An operator with no CLI access can: install a new zombie from a skill template, copy the webhook URL for their trigger, set a cron schedule, configure firewall rules, rename or delete a zombie, and see all of this reflected live in the dashboard. Every action available in `zombiectl zombie *` subcommands has an equivalent UI surface. An agent or pipeline can perform the same operations via the API.

**Problem:** M12 ships a read-only dashboard. The dashboard shows zombie status, activity, and metrics, but offers no way to install or configure zombies. Every setup step requires the CLI: `zombiectl zombie install`, `zombiectl zombie triggers list`, `zombiectl zombie schedule`, `zombiectl zombie firewall set`. This creates a hard dependency on CLI access for anyone who wants to set up or reconfigure a zombie. CTOs, hiring managers, and ops engineers who are not CLI-first users cannot self-serve. They either rely on an engineer or skip UseZombie entirely.

**CLI verb — `install`, not `create`.** Pre-v2.0, the primary zombie-creation command is `zombiectl zombie install`. There is no `create` alias. The verb reflects the operator mental model: "install a zombie (from a skill template) into this workspace." Every reference in this spec uses `install`.

---

## §0 — Route Ownership (M19 vs M27 — no overlap)

**Status:** CONSTRAINT

| Route | Owner | Scope |
|-------|-------|-------|
| `app/(dashboard)/zombies/new/page.tsx` | **M19_001** (this spec) | Install form (template picker + name/desc/skill fields); on submit, POST + redirect; after install, surface webhook URL. |
| `app/(dashboard)/zombies/[id]/page.tsx` | **M27_001** | Detail page — status, kill switch, spend panel, activity feed. M19 does NOT touch this route. |

This is a §-level constraint: if EXECUTE surfaces UI that belongs on `[id]/page.tsx`, it rides in M27, not here. M19's surface ends at the redirect after install.

**Solution summary:** Add the lifecycle CRUD surface to the app. Five panels: (1) Zombie creation form with skill template picker, (2) Trigger panel on zombie detail showing webhook URL with copy button and cron editor, (3) Firewall rules editor with add/edit/delete inline, (4) Zombie config view (rename, change description, delete), (5) Zombie status actions (pause, resume — in addition to kill switch from M12). All API calls use existing backend endpoints that the CLI already calls.

**DX paths:**

| Action | CLI | UI (this milestone) | API |
|---|---|---|---|
| Install zombie | `zombiectl zombie install` | Install form | `POST /v1/workspaces/{ws}/zombies` |
| View webhook URL | `zombiectl zombie triggers list` | Trigger panel, copy button | `GET /v1/workspaces/{ws}/zombies/{id}/triggers` |
| Set cron | `zombiectl zombie schedule` | Cron editor | `POST /v1/workspaces/{ws}/zombies/{id}/schedule` |
| Set firewall | `zombiectl zombie firewall set` | Rules editor | `PUT /v1/workspaces/{ws}/zombies/{id}/firewall` |
| Rename zombie | `zombiectl zombie update` | Config panel inline edit | `PATCH /v1/workspaces/{ws}/zombies/{id}` |
| Delete zombie | `zombiectl zombie delete` | Config panel delete | `DELETE /v1/workspaces/{ws}/zombies/{id}` |

---

## 1.0 Zombie Install Form

**Status:** PENDING

Accessed from Dashboard via "+ Install Zombie" button. Two paths: (a) pick a skill template which pre-fills the name, description, and skill field; (b) start blank. On submit: POST to API, redirect to new zombie's detail page (owned by M27_001).

**Skill-driven picker:** Template options come from `GET /v1/skills` — the same endpoint the onboarding wizard (M25) uses. No hardcoded template list in the frontend. Adding a new skill (e.g., `samples/meeting-maker/`) makes it appear in this picker immediately, zero frontend changes. Each template card shows `display_name` and `description` from `SKILL.md` frontmatter.

**Layout:**

```
+ Install Zombie

┌────────────────────────────────────────────────────┐
│ Choose a template (or start blank)                 │
│                                                    │
│ [Lead Collector] [Blog Writer] [Ops Monitor]       │
│ [Hiring Agent]   [Custom →]                        │
│                                                    │
│ Name          [lead-collector              ]       │
│ Description   [Monitors inbox, scores...   ]       │
│ Skill         [lead-collector-v1       ▾   ]       │
│                                                    │
│              [Cancel]  [Install Zombie →]          │
└────────────────────────────────────────────────────┘
```

**Dimensions:**
- 1.1 PENDING
  - target: `app/(dashboard)/zombies/new/page.tsx`
  - input: `GET /v1/skills` returns skills including "lead-collector"; user selects it
  - expected: name pre-filled as "lead-collector", skill field pre-filled as "lead-collector-v1", description pre-filled from SKILL.md frontmatter
  - test_type: unit (component test, API mock)
- 1.2 PENDING
  - target: `app/(dashboard)/zombies/new/page.tsx`
  - input: user fills form and clicks Install
  - expected: `POST /v1/workspaces/{ws}/zombies` → redirect to `/zombies/{id}` (route owned by M27_001) — new zombie appears in sidebar
  - test_type: integration (API mock)
- 1.3 PENDING
  - target: `app/(dashboard)/zombies/new/page.tsx`
  - input: user submits with empty name
  - expected: client-side validation: "Zombie name is required"
  - test_type: unit (component test)
- 1.4 PENDING
  - target: `app/(dashboard)/zombies/new/page.tsx`
  - input: user submits and API returns 409 (name conflict)
  - expected: toast "A zombie named 'lead-collector' already exists in this workspace"
  - test_type: unit (component test)
- 1.5 PENDING
  - target: `zombiectl/src/commands/zombie_install.js` (CLI success path)
  - input: `zombiectl zombie install` completes successfully (API returns 201 with webhook URL)
  - expected: stdout contains the literal line `🎉 Woohoo! Your zombie is installed and ready to run.` followed by the webhook URL on the next line
  - test_type: unit (CLI test asserting `Woohoo! Your zombie is installed` is present in stdout)

---

## 2.0 Trigger Configuration Panel

**Status:** PENDING

On the Zombie detail page, a "Trigger" section shows how the zombie is invoked. Two modes: webhook (for event-driven zombies) and cron (for scheduled zombies). Webhook mode shows the URL read-only with a copy button. Cron mode shows a schedule editor with preset buttons.

**Layout:**

```
Trigger
┌──────────────────────────────────────────────────────┐
│ ● Webhook (event-driven)   ○ Schedule (cron)         │
│                                                      │
│ Webhook URL                                          │
│ https://api.usezombie.com/v1/webhooks/zom_01xyz      │
│ [Copy]                                               │
│                                                      │
│ Paste this URL into AgentMail, Grafana, Slack Events │
│ API, or any webhook-capable service.                 │
└──────────────────────────────────────────────────────┘

(cron mode)
┌──────────────────────────────────────────────────────┐
│ ○ Webhook   ● Schedule (cron)                        │
│                                                      │
│ [Every hour] [Daily 9am] [Every Tuesday] [Custom]    │
│                                                      │
│ Cron expression: [0 9 * * 2            ]             │
│ Next run: Tuesday Apr 15 at 09:00 UTC               │
│                                                      │
│ Input payload (optional):                            │
│ { "task": "write_post", "topics_source": "..." }     │
│                                                      │
│               [Save Schedule]                        │
└──────────────────────────────────────────────────────┘
```

**Dimensions:**
- 2.1 PENDING
  - target: `app/zombies/[id]/components/TriggerPanel.tsx`
  - input: zombie with webhook trigger
  - expected: webhook URL displayed, copy button copies to clipboard
  - test_type: unit (component test)
- 2.2 PENDING
  - target: `app/zombies/[id]/components/TriggerPanel.tsx`
  - input: user selects cron mode, clicks "Every Tuesday", clicks Save
  - expected: `POST /v1/workspaces/{ws}/zombies/{id}/schedule` with `{ cron: "0 9 * * 2" }` — success toast
  - test_type: integration (API mock)
- 2.3 PENDING
  - target: `app/zombies/[id]/components/TriggerPanel.tsx`
  - input: invalid cron expression entered
  - expected: inline error "Invalid cron expression" — Save disabled
  - test_type: unit (component test)
- 2.4 PENDING
  - target: `app/zombies/[id]/components/TriggerPanel.tsx`
  - input: zombie with cron schedule
  - expected: "Next run" timestamp calculated and shown
  - test_type: unit (component test)

---

## 3.0 Firewall Rules Editor

**Status:** PENDING

Replaces CLI `zombiectl zombie firewall set`. Shows current endpoint rules in a table with inline add/edit/delete. Each rule: method, domain, path, action (allow/block/requires_approval), optional condition.

**Layout:**

```
Firewall Rules
┌──────────────────────────────────────────────────────────────────────┐
│ METHOD  DOMAIN          PATH                   ACTION         COND   │
├──────────────────────────────────────────────────────────────────────┤
│ POST    slack.com       /api/chat.postMessage   Approve if    [edit]│
│                                                 @sales-lead          │
│ POST    api.hubspot.com /crm/v3/contacts         Allow         [edit]│
│ [+ Add rule]                                                   [del] │
└──────────────────────────────────────────────────────────────────────┘

Add rule form (inline):
Method: [POST ▾]  Domain: [slack.com]  Path: [/api/chat.postMessage]
Action: [Requires approval ▾]
Condition (optional): [body.text contains @sales-lead]
[Cancel] [Add Rule]
```

**Dimensions:**
- 3.1 PENDING
  - target: `app/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: zombie with 2 existing firewall rules
  - expected: table renders both rules with method, domain, path, action, condition
  - test_type: unit (component test)
- 3.2 PENDING
  - target: `app/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: user adds a new rule and clicks Add Rule
  - expected: `PUT /v1/workspaces/{ws}/zombies/{id}/firewall` with updated rules array — success toast
  - test_type: integration (API mock)
- 3.3 PENDING
  - target: `app/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: user deletes a rule
  - expected: confirmation dialog → `PUT /v1/workspaces/{ws}/zombies/{id}/firewall` with rule removed
  - test_type: integration (API mock)
- 3.4 PENDING
  - target: `app/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: user submits rule with empty domain
  - expected: client-side validation "Domain is required"
  - test_type: unit (component test)

---

## 4.0 Zombie Config Panel

**Status:** PENDING

Rename, update description, pause/resume, and delete a zombie. Pause is distinct from kill (kill stops a running action; pause prevents new triggers from firing).

**Dimensions:**
- 4.1 PENDING
  - target: `app/zombies/[id]/components/ZombieConfig.tsx`
  - input: user renames zombie from "lead-collector" to "lead-collector-prod"
  - expected: `PATCH /v1/workspaces/{ws}/zombies/{id}` with new name — page title updates
  - test_type: integration (API mock)
- 4.2 PENDING
  - target: `app/zombies/[id]/components/ZombieConfig.tsx`
  - input: user clicks Pause
  - expected: `POST /v1/workspaces/{ws}/zombies/{id}:pause` — status badge updates to "Paused"; new triggers rejected
  - test_type: integration (API mock)
- 4.3 PENDING
  - target: `app/zombies/[id]/components/ZombieConfig.tsx`
  - input: user clicks Delete
  - expected: confirmation dialog shows zombie name and pending action count → `DELETE /v1/workspaces/{ws}/zombies/{id}` → redirect to dashboard
  - test_type: integration (API mock)

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 API Endpoints Consumed

All existing (zombiectl calls these today). UI consumes the same endpoints.

```
POST   /v1/workspaces/{ws}/zombies               — install
PATCH  /v1/workspaces/{ws}/zombies/{id}          — rename / update description
DELETE /v1/workspaces/{ws}/zombies/{id}          — delete
GET    /v1/workspaces/{ws}/zombies/{id}/triggers — get webhook URL
POST   /v1/workspaces/{ws}/zombies/{id}/schedule — set cron
GET    /v1/workspaces/{ws}/zombies/{id}/firewall  — get rules
PUT    /v1/workspaces/{ws}/zombies/{id}/firewall  — set rules
POST   /v1/workspaces/{ws}/zombies/{id}:pause    — pause
POST   /v1/workspaces/{ws}/zombies/{id}:resume   — resume
```

### 5.2 No new backend endpoints

All operations use existing API surface. This is a pure frontend milestone.

### 5.3 Error Contracts

| Error condition | Code | HTTP |
|---|---|---|
| Zombie name conflict | `UZ-ZOM-002` | 409 |
| Invalid cron expression | `UZ-ZOM-003` | 422 |
| Delete while run in progress | `UZ-ZOM-004` | 409 — user should kill first |

---

## 6.0 Implementation Constraints (Enforceable)

| Constraint | How to verify |
|---|---|
| Each component file < 400 lines | `wc -l app/zombies/**/*.tsx` |
| Cron expression validated client-side before API call | Dim 2.3 |
| No credentials in browser state | grep |
| All API errors shown as toast | Component tests |
| Deployed via Vercel (same app as M12) | `vercel deploy` |

---

## 7.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | Zombie creation form + template picker | Dims 1.1–1.4 |
| 2 | Trigger panel (webhook URL + cron editor) | Dims 2.1–2.4 |
| 3 | Firewall rules editor | Dims 3.1–3.4 |
| 4 | Config panel (rename + pause + delete) | Dims 4.1–4.3 |
| 5 | Full test + lint | all dims pass |

---

## 8.0 Acceptance Criteria

- [ ] Install zombie from dashboard — verify: dim 1.2
- [ ] `zombiectl zombie install` prints the literal line `🎉 Woohoo! Your zombie is installed and ready to run.` (followed by the webhook URL) on successful install — verify: dim 1.5
- [ ] Webhook URL copyable from trigger panel — verify: dim 2.1
- [ ] Cron schedule set and next-run shown — verify: dim 2.2 + 2.4
- [ ] Firewall rule added/deleted — verify: dims 3.2 + 3.3
- [ ] Zombie rename works — verify: dim 4.1
- [ ] Zombie delete with confirmation — verify: dim 4.3
- [ ] All API errors shown as toasts — verify: component tests

---

## Applicable Rules

Standard set for Next.js components. RULE FLL (350-line gate).

---

## Eval Commands

```bash
npm run build 2>&1 | head -5; echo "build=$?"
npm run test 2>&1 | tail -5; echo "test=$?"
npm run lint 2>&1 | grep -E "✓|FAIL"
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

---

## Out of Scope

- Skill template editor (editing the YAML prompt template itself — CLI-only for V1)
- Zombie cloning / duplication
- Zombie migration between workspaces
- Webhook secret rotation from UI (use CLI for now)
