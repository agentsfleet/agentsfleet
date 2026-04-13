# M25_001: Zombie Onboarding Wizard — Guided Setup for Common Archetypes

**Prototype:** v2
**Milestone:** M25
**Workstream:** 001
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P2 — Reduces CLI dependency for first-zombie setup; critical for non-engineer operators
**Batch:** B8 — after M19 (lifecycle), M20 (approval inbox), M21 (provider), M22 (grants)
**Branch:** feat/m25-onboarding-wizard
**Depends on:** M19_001 (zombie creation + trigger), M22_001 (grants UI), M20_001 (approval inbox), M21_001 (provider), M13_001 (credential vault), M11_003 (signup)

---

## Overview

**Goal (testable):** A newly signed-up operator who has never used the CLI can: (1) see a "Set up your first zombie" CTA on the empty dashboard, (2) click it, (3) pick "Lead Collector" from the archetype picker, (4) complete a 7-step wizard that creates the zombie, adds required credentials, copies the webhook URL, requests grants, configures approval gates, selects a provider, and launches — within 10 minutes with no documentation needed. The wizard state is persisted so the operator can pause at any step and resume later.

**Problem:** After signup (M25), the operator lands on an empty dashboard. The product is now completely self-serve, but "completely self-serve" for a zombie setup currently means running ~8 distinct CLI commands and understanding the grant flow, the firewall rule syntax, and the trigger webhook mechanism. Most operators who are not engineers will not do this. The wizard wraps all of M19, M20, M22, and M13's primitives into a single guided flow that matches the mental model of the archetype guides (hiring_agent_zombie.md, lead_collector_zombie.md, ops_zombie.md).

**Solution summary:** A multi-step wizard component that: (a) presents archetype-specific setup flows, (b) pre-fills all options using archetype templates, (c) persists progress to the server so partially completed wizards survive page refresh, (d) shows the CLI equivalent of each step for power users, (e) completes by launching the zombie's first run or showing the trigger URL to configure externally.

---

## 1.0 Post-Signup Entry Point

**Status:** PENDING

When a user logs in for the first time (workspace has 0 zombies), the dashboard shows an onboarding prompt instead of the empty state.

**Layout:**

```
Welcome to UseZombie.
You have 20 free runs. Let's set up your first zombie.

[Lead Collector]   [Blog Writer]   [Ops Monitor]
[Hiring Agent]     [Meeting Maker] [Custom →]

─────────────────────────────────────────────────
Lead Collector                        Most Popular
───────────────────────────────────────────────
Monitors your inbox, scores leads,
routes to CRM and Slack. Requires:
  ✓ AgentMail or Gmail
  ✓ HubSpot or Pipedrive
  ✓ Slack

[Start setup →]
```

**Dimensions:**
- 1.1 PENDING
  - target: `app/dashboard/page.tsx`
  - input: authenticated user with workspace containing 0 zombies
  - expected: onboarding prompt renders with archetype picker; not the normal dashboard layout
  - test_type: integration (API mock)
- 1.2 PENDING
  - target: `app/dashboard/page.tsx`
  - input: workspace with 1+ zombies
  - expected: normal dashboard layout (not the wizard CTA)
  - test_type: unit (component test)

---

## 2.0 Wizard Steps (Lead Collector reference flow)

**Status:** PENDING

7 steps, each with a "What's this?" tooltip, a progress bar, and an expandable "CLI equivalent" sidebar. Steps are individually completable and re-entrant — the wizard saves state after each step.

**Step 1 — Name your zombie:**

```
Step 1 of 7: Name your zombie

Name:         [lead-collector        ]
Description:  [Monitors inbox, scores leads, routes to CRM and Slack]

                              [Next →]

CLI: zombiectl zombie create --name "lead-collector" --skill lead-collector-v1
```

**Step 2 — Add credentials:**

```
Step 2 of 7: Add credentials

Lead Collector needs access to these services:

  [✓] AgentMail      sk_live_...      [Added]
  [✓] HubSpot        hs_...           [Added]
  [ ] Slack          —                [+ Add]

You can add missing credentials now or skip and add them later.
Credentials stored encrypted — values are write-only.

                     [← Back]  [Next →]
```

**Step 3 — Configure trigger:**

```
Step 3 of 7: Configure your trigger

Lead Collector fires when an email arrives.

Webhook URL:
https://api.usezombie.com/v1/webhooks/zom_01xyz  [Copy]

Paste this URL into AgentMail:
  AgentMail dashboard → Inbox → Webhooks → Add
  Event: message.received

                     [← Back]  [I've configured this →]

CLI: zombiectl zombie triggers list --zombie zom_01xyz
```

**Step 4 — Request integration grants:**

```
Step 4 of 7: Request access to services

Lead Collector will call these services on your behalf.
Approve each request to authorize it.

  [✓] HubSpot   — Approved 2 min ago
  [⏳] Slack    — Approval request sent. Check Slack DM.
                  [Re-send] [Already approved →]

                     [← Back]  [Next →]

CLI: zombiectl grant request --zombie zom_01xyz --service hubspot
```

**Step 5 — Configure approval gates:**

```
Step 5 of 7: Set up approval gates

Approval gates let you review high-stakes actions before they execute.
We've pre-configured safe defaults for Lead Collector:

  ✓ Require approval when tagging @sales-lead in Slack
    (hot leads, score ≥ 90)

  + Add custom rule

                     [← Back]  [Next →]

CLI: zombiectl zombie firewall set --zombie zom_01xyz --config '{...}'
```

**Step 6 — Choose your LLM provider:**

```
Step 6 of 7: LLM Provider

How should Lead Collector think?

● UseZombie hosted (20 free runs included)
○ Bring your own key (Anthropic, OpenAI)

                     [← Back]  [Next →]

CLI: zombiectl provider get
```

**Step 7 — Review and launch:**

```
Step 7 of 7: Review and launch

lead-collector is ready.

  ✓ Zombie created
  ✓ Credentials: AgentMail, HubSpot, Slack
  ✓ Trigger: Webhook (configured)
  ✓ Grants: HubSpot ✓ · Slack ✓
  ✓ Approval gate: @sales-lead mentions
  ✓ Provider: UseZombie hosted (20 runs)

When an email arrives at your AgentMail inbox,
lead-collector will score it and route it automatically.

Check your Approval Inbox when hot leads come in.

                     [← Back]  [Launch →]
```

**Dimensions:**
- 2.1 PENDING
  - target: `app/onboarding/wizard/page.tsx`
  - input: user completes all 7 steps for Lead Collector archetype
  - expected: zombie created, credentials added, trigger configured, grants requested, gates set, provider set; final step shows all green checkmarks
  - test_type: e2e
- 2.2 PENDING
  - target: wizard progress persistence
  - input: user completes steps 1–3, closes browser, reopens wizard
  - expected: wizard resumes at step 4 with steps 1–3 marked complete
  - test_type: integration (API mock)
- 2.3 PENDING
  - target: `app/onboarding/wizard/page.tsx`
  - input: user skips Step 2 (missing Slack credential) and completes wizard
  - expected: wizard completes with warning: "Slack credential missing — Slack notifications won't work until you add it in Credentials"
  - test_type: unit (component test)
- 2.4 PENDING
  - target: CLI sidebar
  - input: user clicks "Show CLI" on any step
  - expected: sidebar shows exact CLI command equivalent for that step (pre-filled with actual zombie ID and values)
  - test_type: unit (component test)

---

## 3.0 Skill-Driven Wizard

**Status:** PENDING

There is no separate "archetype" concept. A zombie's archetype IS its skill. Each skill in `samples/{slug}/` has two files:
- `SKILL.md` — frontmatter (name, description, tags) + agent prompt
- `TRIGGER.md` — frontmatter with `trigger`, `credentials`, `network`, `budget`

The wizard reads `TRIGGER.md` to pre-fill every step. No separate catalog, no wizard-specific config files.

**TRIGGER.md for lead-collector (already exists):**

```yaml
name: lead-collector
trigger:
  type: webhook
  source: agentmail
  event: message.received
skills:
  - agentmail
credentials:
  - agentmail_api_key
network:
  allow:
    - api.agentmail.to
budget:
  daily_dollars: 5.00
  monthly_dollars: 29.00
```

**How wizard steps derive from TRIGGER.md:**

| Wizard step | Source field |
|---|---|
| Step 1 (name) | `name` from SKILL.md frontmatter |
| Step 2 (credentials) | `credentials` list |
| Step 3 (trigger) | `trigger.type` + `trigger.source` + `trigger.event` |
| Step 4 (grants) | `credentials` list (same services need grants) |
| Step 5 (firewall rules) | `network.allow` → converted to `allow` rules; operator can add `requires_approval` conditions |
| Step 6 (provider/budget) | `budget.daily_dollars` as spend limit default |

**API: wizard skill picker uses the skills endpoint:**
```
GET /v1/skills              — list skills (slug, display_name, description from SKILL.md frontmatter)
GET /v1/skills/{slug}       — full skill: SKILL.md body + TRIGGER.md fields parsed
```

Adding a new skill (e.g., meeting-maker) = adding `samples/meeting-maker/SKILL.md` + `TRIGGER.md`. It appears in the wizard picker immediately. Zero frontend changes. Zero new config files.

**Dimensions:**
- 3.1 PENDING
  - target: skill picker in wizard
  - input: `GET /v1/skills` returns skills including "ops-monitor"
  - expected: picker renders all skills; selecting ops-monitor fetches `GET /v1/skills/ops-monitor`; wizard step 2 pre-fills credentials from TRIGGER.md; step 3 pre-fills trigger type; step 5 pre-fills network.allow rules
  - test_type: integration (API mock)
- 3.2 PENDING
  - target: new skill addition
  - input: add `samples/meeting-maker/SKILL.md` + `TRIGGER.md` to backend
  - expected: `GET /v1/skills` returns meeting-maker; picker shows it; wizard pre-fills from its TRIGGER.md — zero frontend changes
  - test_type: integration (live backend)

---

## 4.0 Interfaces

**Status:** PENDING

### 4.1 New API Endpoints

**Skills catalog (no auth required — lists available zombie skills):**
```
GET /v1/skills                                — list skills (slug, display_name, description from SKILL.md frontmatter)
GET /v1/skills/{slug}                         — full skill: SKILL.md body + TRIGGER.md fields (credentials, trigger, network, budget)
```

**Wizard state (workspace-scoped, requires auth):**
```
GET    /v1/workspaces/{ws}/wizard             — get current wizard state (or null if none)
PUT    /v1/workspaces/{ws}/wizard             — upsert wizard state (persist progress)
DELETE /v1/workspaces/{ws}/wizard             — clear wizard state (on complete or dismiss)
```

### 4.2 Wizard State Schema

```json
{
  "archetype": "lead-collector",
  "current_step": 4,
  "completed_steps": [1, 2, 3],
  "zombie_id": "zom_01xyz",
  "data": {
    "name": "lead-collector",
    "credentials_added": ["agentmail", "hubspot"],
    "trigger_copied": true,
    "grants_requested": ["hubspot", "slack"]
  }
}
```

### 4.3 Existing Endpoints Used

All creation/configuration calls go through M19, M20, M22, M13 endpoints. The wizard is an orchestration layer — it calls the same APIs as if the user were doing each step manually.

---

## 5.0 Implementation Constraints

| Constraint | How to verify |
|---|---|
| Adding a new skill requires only SKILL.md + TRIGGER.md in `samples/` — zero frontend changes | Dim 3.2 |
| Wizard frontend contains no hardcoded skill/credential data — all pre-fill comes from `GET /v1/skills/{slug}` | grep for credential ref literals in wizard components |
| Wizard state persisted after each step (browser refresh safe) | Dim 2.2 |
| CLI equivalent shown for every step | Dim 2.4 |
| Each wizard component < 400 lines | `wc -l` |
| Wizard only available if workspace has 0 completed zombies OR user navigates to /onboarding | Dim 1.2 |

---

## 6.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | Backend: extend `GET /v1/skills/{slug}` to parse and return TRIGGER.md fields (credentials, trigger, network, budget) | Dim 3.2 |
| 2 | Verify `samples/lead-collector/TRIGGER.md` parses correctly; add TRIGGER.md to any skills that are missing it | Dim 3.1 |
| 3 | Wizard state API (persist/restore) | Dim 2.2 |
| 4 | Wizard steps 1–4 (create, credentials, trigger, grants) | Dims 2.1 partial |
| 5 | Wizard steps 5–7 (gates, provider, launch) | Dim 2.1 complete |
| 6 | CLI equivalent sidebar | Dim 2.4 |
| 7 | Empty dashboard CTA | Dims 1.1–1.2 |
| 8 | Full e2e test (Lead Collector end-to-end) | Dim 2.1 |

---

## 7.0 Acceptance Criteria

- [ ] Lead Collector archetype: 7-step wizard completes, zombie launches — verify: dim 2.1 e2e
- [ ] Wizard survives page refresh at step 4 — verify: dim 2.2
- [ ] Missing credential: warning, not block — verify: dim 2.3
- [ ] New archetype added via template only — verify: dim 3.2
- [ ] CLI equivalent shown on every step — verify: dim 2.4
- [ ] Empty dashboard shows wizard CTA — verify: dim 1.1

---

## Applicable Rules

Standard Next.js set. RULE FLL (400-line gate for components).

---

## Out of Scope

- Custom skill template creation from wizard (power-user feature, CLI-only)
- Multi-zombie setup in a single wizard session
- Onboarding video or interactive tutorial overlay
- Zombie testing/dry-run from wizard (future)
- Memory seeding from wizard (use Memory tab after setup — M24)
