# M19_001: Zombie Lifecycle UI — Install, Configure, and Manage Zombies from the Dashboard

**Prototype:** v2
**Milestone:** M19
**Workstream:** 001
**Date:** Apr 13, 2026 (amended Apr 22, 2026)
**Status:** IN_PROGRESS
**Priority:** P1 — Without this, the dashboard is read-only; operators must use the CLI for all setup
**Batch:** B2 — alpha gate, parallel with M11_005 (done), M13_001, M21_001, M27_001, M31_001, M33_001
**Branch:** feat/m19-zombie-lifecycle-ui
**Depends on:** M12_001 (app shell + layout — done), M9_001 (grants/execute API — done), M11_006 (tenant billing `is_exhausted` — in progress; §5 reads its field once merged)

---

## §0.5 — Amendment (Apr 22, 2026): Backend Surface Verified, Scope Reduced to Buildable

**Status:** CONSTRAINT (supersedes conflicting statements in §1–§5 below; original text retained for traceability)

Before CHORE(open) the backend route manifest (`src/http/route_manifest.zig`) was verified against every API endpoint this spec assumes. The original spec's claim "All existing (zombiectl calls these today). UI consumes the same endpoints. This is a pure frontend milestone" is **wrong**: zombiectl itself only implements `install` / `list` / `delete` / `activity`; the remaining commands (`schedule`, `firewall set`, `triggers list`, `update`, `pause`, `resume`) do not exist in CLI or backend. Scope is trimmed to what is buildable today; deferred items are named here with their unblocking milestone.

### Backend surface available now (buildable in this workstream)

| Endpoint | In manifest? | M19 use |
|---|---|---|
| `POST   /v1/workspaces/{ws}/zombies` | ✓ | §1 install |
| `GET    /v1/workspaces/{ws}/zombies` | ✓ | zombies list page (scaffolding) |
| `DELETE /v1/workspaces/{ws}/zombies/{id}` | ✓ | §4 delete |
| `POST   /v1/workspaces/{ws}/zombies/{id}/stop` | ✓ | kill (already wired by M12 detail page in M27 stub) |
| `GET    /v1/workspaces/{ws}/zombies/{id}/activity` | ✓ | detail composition (M27 final owner) |
| `POST   /v1/webhooks/{zombie_id}` | ✓ | §2 webhook URL derived client-side (URL template is deterministic) |
| `GET    /v1/tenants/me/billing` | ✓ | §5 exhaustion UI (reads `is_exhausted` / `exhausted_at` after M11_006 merges) |

### Backend surface **missing** — deferred with named follow-up

| Missing endpoint | Original §/Dim | Deferred to |
|---|---|---|
| `GET /v1/skills` (template picker source) | §1 template picker, dim 1.1 | M19_002 (skills catalog UI + backend) |
| `PATCH /v1/workspaces/{ws}/zombies/{id}` (rename / describe) | §4 dim 4.1 | M19_003 (zombie mutation endpoints) |
| `POST .../{id}/schedule` (cron) | §2 cron mode, dims 2.2–2.4 | M19_003 |
| `GET \| PUT .../{id}/firewall` | §3 entire section | M19_004 (firewall UI — paired with a backend firewall REST surface; CLI-only for V1) |
| `POST .../{id}:pause` / `:resume` | §4 dim 4.2 | M19_003 |
| `GET .../{id}/triggers` (webhook URL fetch) | §2 dim 2.1 | Not needed — URL is `${API_BASE}/v1/webhooks/${zombie_id}`, derivable client-side. Dim 2.1 stays, rewritten. |

### Route ownership bridge (M19 ↔ M27, unchanged in spirit)

§0's partition remains: M19 owns `zombies/new/page.tsx` + `zombies/[id]/components/*.tsx`. M27 owns `zombies/[id]/page.tsx`. Because M27 is PENDING on M26_001 (design-system unification) and M19 needs a host for its panel components, **M19 additionally ships a minimal `zombies/[id]/page.tsx` stub** that composes the three M19 panels and a status header. The stub is tagged `// TODO(M27_001): replace with full detail page composition (kill switch, spend panel, activity feed).` When M27 lands, its `page.tsx` supersedes the stub; M19's `components/` tree is imported unchanged. Same for `zombies/page.tsx` (list) — M19 scaffolds a minimal list, M27 replaces.

### Revised §1–§5 dimension status (authoritative)

| Original dim | Status after amendment | Action |
|---|---|---|
| 1.1 (template picker pre-fill) | DEFERRED → M19_002 | no `/v1/skills`; install form uses blank fields only |
| 1.2 (install + redirect) | IN SCOPE | unchanged |
| 1.3 (empty-name validation) | IN SCOPE | unchanged |
| 1.4 (409 conflict toast) | IN SCOPE | unchanged |
| 1.5 (CLI `Woohoo!` line) | IN SCOPE | zombiectl edit only |
| 2.1 (webhook URL + copy) | IN SCOPE, REWRITTEN | URL derived client-side from `zombie_id` — no backend fetch |
| 2.2, 2.3, 2.4 (cron) | DEFERRED → M19_003 | cron UI rendered as disabled tab with "CLI-only for V1" message |
| 3.1–3.4 (firewall editor) | DEFERRED → M19_004 | §3 becomes a read-only placeholder panel |
| 4.1 (rename) | DEFERRED → M19_003 | |
| 4.2 (pause) | DEFERRED → M19_003 | |
| 4.3 (delete + confirmation) | IN SCOPE | unchanged |
| 5.1 NEW | IN SCOPE | exhaustion banner on dashboard overview (reads `is_exhausted`) |
| 5.2 NEW | IN SCOPE | per-zombie badge on detail stub when tenant is exhausted |

### §5 — Balance exhaustion UI (new, absorbs M11_006 handoff)

M11_006 Out of Scope line: *"UI banners / dashboard affordances for exhausted state — separate M19 workstream pulls `is_exhausted` into the UI."* Absorbed here so the handoff doesn't drift.

**Dimensions:**

- 5.1 IN SCOPE
  - target: `app/(dashboard)/page.tsx` (dashboard overview)
  - input: `GET /v1/tenants/me/billing` returns `{is_exhausted: true, exhausted_at: <epoch_ms>}`
  - expected: a destructive-tone banner renders above the main content with "Your credit balance is exhausted. Runs are paused/warn/continue per `BALANCE_EXHAUSTED_POLICY`. [Contact support]" and a timestamp.
  - test_type: unit (component test, API mock)
- 5.2 IN SCOPE
  - target: `app/(dashboard)/zombies/[id]/page.tsx` (stub; M27 will preserve this behavior)
  - input: same API response
  - expected: a "Balance exhausted" badge renders adjacent to the zombie name in the detail header.
  - test_type: unit (component test)
- 5.3 IN SCOPE
  - target: both surfaces
  - input: `is_exhausted: false`
  - expected: no banner, no badge, no layout shift.
  - test_type: unit (component test)

### §6 — Scaffolding (new)

Before any panel work. Creates the route skeleton so M27's eventual detail page has neighbors and M19's install redirect has a destination.

**Dimensions:**

- 6.1 IN SCOPE
  - target: `app/(dashboard)/zombies/page.tsx`
  - expected: renders a minimal list using `GET /v1/workspaces/{ws}/zombies` via the resolved active-workspace context; "+ Install Zombie" button links to `new/`. Marked `// TODO(M27_001): full list with status + spend columns.`
  - test_type: unit (component test, API mock)
- 6.2 IN SCOPE
  - target: `app/(dashboard)/zombies/[id]/page.tsx` (stub)
  - expected: renders `<ZombieConfig>` + `<TriggerPanel>` + `<FirewallRulesEditor>` and the exhaustion badge from §5.2. TODO comment points to M27_001.
  - test_type: unit (smoke — component renders without crashing)
- 6.3 IN SCOPE
  - target: `components/layout/Shell.tsx`
  - expected: sidebar gains a "Zombies" link pointing at `/zombies`.
  - test_type: unit

### Revised §5.1 — API endpoints actually consumed

```
POST   /v1/workspaces/{ws}/zombies               — install (§1)
GET    /v1/workspaces/{ws}/zombies               — list (§6.1)
DELETE /v1/workspaces/{ws}/zombies/{id}          — delete (§4.3)
GET    /v1/tenants/me/billing                    — exhaustion UI (§5)
```

Webhook URL (§2) is **derived client-side** — not an API call. `POST /v1/webhooks/{zombie_id}` is the public ingress path the customer pastes into AgentMail/Slack/etc.

---

## Overview

**Goal (testable):** An operator with no CLI access can: install a new zombie from a skill template, copy the webhook URL for their trigger, set a cron schedule, configure firewall rules, rename or delete a zombie, and see all of this reflected live in the dashboard. Every action available in `zombiectl zombie *` subcommands has an equivalent UI surface. An agent or pipeline can perform the same operations via the API.

> **Amendment note:** the goal paragraph above is the *original* aspiration. The Apr 22 amendment (§0.5) trims the shippable surface for this workstream to install + delete + webhook copy + exhaustion UI + route scaffolding; rename / pause / resume / cron / firewall editor / template picker move to the M19_002 – M19_004 follow-ups listed in §0.5.

**Problem:** M12 ships a read-only dashboard. The dashboard shows zombie status, activity, and metrics, but offers no way to install or configure zombies. Every setup step requires the CLI: `zombiectl zombie install`, `zombiectl zombie triggers list`, `zombiectl zombie schedule`, `zombiectl zombie firewall set`. This creates a hard dependency on CLI access for anyone who wants to set up or reconfigure a zombie. CTOs, hiring managers, and ops engineers who are not CLI-first users cannot self-serve. They either rely on an engineer or skip UseZombie entirely.

**CLI verb — `install`, not `create`.** Pre-v2.0, the primary zombie-creation command is `zombiectl zombie install`. There is no `create` alias. The verb reflects the operator mental model: "install a zombie (from a skill template) into this workspace." Every reference in this spec uses `install`.

---

## §0 — Route Ownership (M19 vs M27 — file-level partition, no overlap)

**Status:** CONSTRAINT

| Path | Owner | Scope |
|------|-------|-------|
| `app/(dashboard)/zombies/new/page.tsx` | **M19_001** (this spec) | Install form (template picker + name/desc/skill fields); on submit, POST + redirect to `/zombies/{id}`; after install, surface webhook URL. |
| `app/(dashboard)/zombies/[id]/page.tsx` | **M27_001** | Detail page **file** — composes M19's panel components below, adds M27-owned widgets (status header, kill switch, spend panel, activity feed). M19 does NOT edit this file. |
| `app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx` | **M19_001** | Trigger config panel (§2) — webhook URL + copy button, cron editor. Imported and rendered by M27's `page.tsx`. |
| `app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx` | **M19_001** | Firewall rules editor (§3) — rule list + inline add/edit/delete. Imported and rendered by M27's `page.tsx`. |
| `app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx` | **M19_001** | Rename / describe / delete panel (§4). Imported and rendered by M27's `page.tsx`. |

**Partition rule:** M19 owns **lifecycle-behavior components** (install, trigger, firewall, config) — every action maps 1:1 to a `zombiectl zombie *` subcommand, so behavior belongs in this milestone. M27 owns the **detail-page composition** — layout, kill switch, spend panel, activity feed — and imports M19's components as panels.

Path format: always `app/(dashboard)/zombies/[id]/...` with the route group. Any reference dropping the `(dashboard)` segment is a typo — the filesystem path includes it.

If EXECUTE produces file-level surface that isn't in the table above, amend the spec before editing. Merge-conflict risk is eliminated because M19 only ever creates files under `components/` and `new/page.tsx`; M27 only ever creates `page.tsx` plus its own widgets (kill switch, spend panel, activity feed).

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
  - target: `app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx`
  - input: zombie with webhook trigger
  - expected: webhook URL displayed, copy button copies to clipboard
  - test_type: unit (component test)
- 2.2 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx`
  - input: user selects cron mode, clicks "Every Tuesday", clicks Save
  - expected: `POST /v1/workspaces/{ws}/zombies/{id}/schedule` with `{ cron: "0 9 * * 2" }` — success toast
  - test_type: integration (API mock)
- 2.3 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx`
  - input: invalid cron expression entered
  - expected: inline error "Invalid cron expression" — Save disabled
  - test_type: unit (component test)
- 2.4 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/TriggerPanel.tsx`
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
  - target: `app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: zombie with 2 existing firewall rules
  - expected: table renders both rules with method, domain, path, action, condition
  - test_type: unit (component test)
- 3.2 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: user adds a new rule and clicks Add Rule
  - expected: `PUT /v1/workspaces/{ws}/zombies/{id}/firewall` with updated rules array — success toast
  - test_type: integration (API mock)
- 3.3 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: user deletes a rule
  - expected: confirmation dialog → `PUT /v1/workspaces/{ws}/zombies/{id}/firewall` with rule removed
  - test_type: integration (API mock)
- 3.4 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/FirewallRulesEditor.tsx`
  - input: user submits rule with empty domain
  - expected: client-side validation "Domain is required"
  - test_type: unit (component test)

---

## 4.0 Zombie Config Panel

**Status:** PENDING

Rename, update description, pause/resume, and delete a zombie. Pause is distinct from kill (kill stops a running action; pause prevents new triggers from firing).

**Dimensions:**
- 4.1 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx`
  - input: user renames zombie from "lead-collector" to "lead-collector-prod"
  - expected: `PATCH /v1/workspaces/{ws}/zombies/{id}` with new name — page title updates
  - test_type: integration (API mock)
- 4.2 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx`
  - input: user clicks Pause
  - expected: `POST /v1/workspaces/{ws}/zombies/{id}:pause` — status badge updates to "Paused"; new triggers rejected
  - test_type: integration (API mock)
- 4.3 PENDING
  - target: `app/(dashboard)/zombies/[id]/components/ZombieConfig.tsx`
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
