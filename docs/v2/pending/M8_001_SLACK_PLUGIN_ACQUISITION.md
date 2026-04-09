# M8_001: Slack Plugin Acquisition — "Add UseZombie to Slack" → Zombie live in 5 minutes

**Prototype:** v0.9.0
**Milestone:** M8
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** PENDING
**Priority:** P0 — Primary distribution channel; zero-friction onboarding
**Batch:** B4 — after M4 (approval gate in Slack), M6 (firewall)
**Branch:** feat/m8-slack-plugin
**Depends on:** M4_001 (approval gate Slack integration), M3_001 (Slack tool)

---

## Overview

**Goal (testable):** A team clicks "Add UseZombie to Slack" from usezombie.com. OAuth flow completes. UseZombie bot posts in the team's Slack: "What should your zombie do?" Team picks from templates (fix bugs from #bugs, review PRs, manage leads). UseZombie asks for GitHub/email credentials (stored in vault via secure flow). Zombie is live and working in 5 minutes. Approval gates fire in the same Slack workspace. Other team members see the Zombie working — natural viral expansion.

**Problem:** Today, onboarding requires CLI: `zombiectl install`, `zombiectl credential add`, `zombiectl up`. This works for developers but excludes engineering managers, team leads, and non-CLI users. The CEO plan identifies Slack as the primary acquisition channel because every dev team already has it. The CLI path stays for power users, but the Slack path is the growth engine. Without it, UseZombie depends on developer word-of-mouth (slow) instead of team-viral adoption (fast).

**Solution summary:** Build a Slack App with OAuth 2.0 V2 flow (`chat:write`, `commands`, `incoming-webhook`, `channels:read`, `channels:history`, `reactions:write`). On install: create a UseZombie workspace mapped to the Slack team, post onboarding flow as a Slack message with interactive blocks. Template selection creates the Zombie config. Credential collection uses Slack's modal dialog → values sent to UseZombie API → stored in vault. Zombie starts automatically. All approval gates route to the installing workspace. Bot presence shows Zombie is working.

---

## 1.0 Slack App OAuth Flow

**Status:** PENDING

Implement Slack OAuth 2.0 V2 flow. New endpoints: `GET /v1/slack/install` (redirect to Slack authorize URL), `GET /v1/slack/callback` (exchange code for token, create workspace, store bot token in vault). The bot token is a workspace-level credential — stored in the UseZombie vault, never exposed to any Zombie. Scopes requested: `chat:write`, `commands`, `incoming-webhook`, `channels:read`, `channels:history`, `reactions:write`, `users:read`.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleInstall`
  - input: `GET /v1/slack/install`
  - expected: `HTTP 302 redirect to slack.com/oauth/v2/authorize with correct client_id, scopes, redirect_uri`
  - test_type: unit
- 1.2 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `GET /v1/slack/callback?code=xxx&state=yyy` with valid state
  - expected: `Exchange code for bot token, create workspace in DB, store bot token in vault, redirect to success page`
  - test_type: integration (DB + vault mock + HTTP mock for Slack API)
- 1.3 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `GET /v1/slack/callback with invalid state (CSRF)`
  - expected: `HTTP 403, token exchange NOT attempted`
  - test_type: unit
- 1.4 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `GET /v1/slack/callback for team that already has UseZombie installed`
  - expected: `Existing workspace reused (not duplicated), bot token updated`
  - test_type: integration (DB)

---

## 2.0 Onboarding Flow (Interactive Slack Messages)

**Status:** PENDING

After OAuth completes, the bot posts an onboarding message to the team's #general (or configured channel) using Slack Block Kit. The message includes: welcome text, template picker (button group), and setup instructions. Template selection triggers a modal dialog for credential collection.

Flow:
```
Bot: "UseZombie is installed! What should your zombie do?"
     [Fix bugs from #bugs]  [Review PRs]  [Manage leads]  [Custom]

User clicks [Fix bugs from #bugs]

Bot opens modal:
  "Slack Bug Fixer needs:"
  - GitHub repo URL: [___________]
  - GitHub token:    [___________]  (stored in vault, never visible again)
  - Watch channel:   [#bugs ▾]
  [Set up Zombie]

User fills in, clicks [Set up Zombie]

Bot: "Slack Bug Fixer is live! Watching #bugs for bug reports.
      Try it: post a bug in #bugs and watch the magic.
      Approvals will appear here when the Zombie needs permission."
```

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/zombie/slack_onboarding.zig:buildWelcomeMessage`
  - input: `team_name="Acme Corp", available_templates=["slack-bug-fixer", "lead-collector", "pr-reviewer"]`
  - expected: `Slack Block Kit JSON with header, description, and 4 buttons (3 templates + Custom)`
  - test_type: unit
- 2.2 PENDING
  - target: `src/http/handlers/slack_interactions.zig:handleInteraction`
  - input: `Slack button click payload: action_id="template_slack-bug-fixer"`
  - expected: `Slack modal opened via views.open API with credential input fields`
  - test_type: integration (HTTP mock)
- 2.3 PENDING
  - target: `src/http/handlers/slack_interactions.zig:handleModalSubmit`
  - input: `Modal submission with repo_url, github_token, channel_id`
  - expected: `Zombie config created, credentials stored in vault, Zombie started, confirmation message posted`
  - test_type: integration (DB + vault + HTTP mock)
- 2.4 PENDING
  - target: `src/http/handlers/slack_interactions.zig:handleModalSubmit`
  - input: `Modal submission with invalid repo URL`
  - expected: `Slack modal error displayed: "Invalid repo URL. Use format: https://github.com/org/repo"`
  - test_type: unit

---

## 3.0 Workspace ↔ Slack Team Mapping

**Status:** PENDING

Map each Slack team (workspace) to a UseZombie workspace. On first OAuth install: create a new UseZombie workspace with `source=slack`, `slack_team_id`, and `slack_channel_id`. If the team already has a UseZombie workspace (re-install or token refresh), reuse it. The mapping is stored in `core.workspaces` (add `slack_team_id` column). Bot token stored as a workspace-level credential in vault: `op://ZMB_LOCAL_DEV/slack_bot_{team_id}/token`.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `src/zombie/slack_onboarding.zig:findOrCreateWorkspace`
  - input: `slack_team_id="T01ABC", team_name="Acme Corp", no existing workspace`
  - expected: `New workspace created with slack_team_id set, workspace_id returned`
  - test_type: integration (DB)
- 3.2 PENDING
  - target: `src/zombie/slack_onboarding.zig:findOrCreateWorkspace`
  - input: `slack_team_id="T01ABC", workspace already exists`
  - expected: `Existing workspace returned, no duplicate`
  - test_type: integration (DB)
- 3.3 PENDING
  - target: `schema/025_add_slack_team_id.sql`
  - input: `ALTER TABLE core.workspaces ADD slack_team_id TEXT`
  - expected: `Column added, nullable (CLI-created workspaces won't have it)`
  - test_type: integration (DB)

---

## 4.0 Slack Event Routing

**Status:** PENDING

Route Slack Events API payloads to the correct Zombie based on channel and team. When a message arrives in #bugs for team T01ABC: look up workspace by slack_team_id → find Zombie configured to watch that channel → enqueue event. This extends M3's webhook handler to support Slack-installed Zombies (which don't use the generic `/v1/webhooks/{zombie_id}` endpoint — they receive events via the Slack Events API subscription).

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `Slack event_callback: team_id="T01ABC", channel="C_BUGS", type="message"`
  - expected: `Look up workspace by team → find Zombie watching C_BUGS → enqueue event`
  - test_type: integration (DB + Redis)
- 4.2 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `Slack event for channel with no Zombie`
  - expected: `Event ignored (HTTP 200, no enqueue)`
  - test_type: integration (DB)
- 4.3 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `Slack event from bot message (to prevent loops)`
  - expected: `Event ignored (don't process our own bot messages)`
  - test_type: unit

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 API Endpoints

```
GET  /v1/slack/install                    — redirect to Slack OAuth
GET  /v1/slack/callback                   — OAuth code exchange
POST /v1/slack/events                     — Slack Events API receiver
POST /v1/slack/interactions               — Slack interactive message callbacks
```

### 5.2 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| OAuth state mismatch | `UZ-SLACK-001` | "OAuth state invalid. Please try installing again." | 403 |
| Token exchange failed | `UZ-SLACK-002` | "Could not connect to Slack. Try again." | 502 |
| Team already installed | -- | Reuse workspace (not an error) | -- |
| Modal validation failed | -- | Slack modal error inline | -- |
| Bot token expired | `UZ-SLACK-003` | "Slack token expired. Please reinstall UseZombie." | -- |

---

## 6.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| OAuth callback never arrives | User cancels Slack auth | No workspace created, install page shows retry | "Installation cancelled" |
| Bot removed from Slack | Team admin uninstalls | Zombie pauses, activity: "Slack bot removed" | Zombie stops |
| Token revoked | Security rotation | API calls fail, Zombie pauses | Activity: "Slack token expired" |
| Rate limit during onboarding | Many teams installing simultaneously | Retry with backoff | Brief delay in bot response |
| Credential modal abandoned | User closes modal | No Zombie created, bot posts "Need help? Try again." | Gentle nudge |

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| OAuth state uses cryptographic random + HMAC | Code review |
| Bot token stored in vault, never in DB or logs | grep for token pattern in codebase |
| Slack signing secret verified on all endpoints | Unit tests |
| CSRF protection on OAuth flow | Test 1.3 |
| Schema migration file ≤ 100 lines | `wc -l` |
| Cross-compiles | both targets |

---

## 8.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Schema migration: add slack_team_id to workspaces | `zig build` compiles |
| 2 | Implement OAuth flow (install + callback) | Tests 1.1-1.4 pass |
| 3 | Implement onboarding message (Block Kit) | Test 2.1 pass |
| 4 | Implement modal interaction handler | Tests 2.2-2.4 pass |
| 5 | Implement workspace ↔ team mapping | Tests 3.1-3.2 pass |
| 6 | Implement Slack event routing | Tests 4.1-4.3 pass |
| 7 | Register Slack app in Slack API (manual config) | Bot responds to test message |
| 8 | Full test suite | `make test && make test-integration && make lint` |

---

## 9.0 Acceptance Criteria

**Status:** PENDING

- [ ] OAuth flow completes: team installs, workspace created, token in vault — verify: integration test
- [ ] Onboarding message appears with template picker — verify: unit test
- [ ] Credential modal collects and stores secrets — verify: integration test
- [ ] Zombie starts automatically after onboarding — verify: integration test
- [ ] Slack events route to correct Zombie — verify: integration test
- [ ] Bot messages don't trigger loops — verify: unit test
- [ ] Re-install reuses workspace — verify: integration test
- [ ] `make test && make lint` pass
- [ ] Cross-compile passes

---

## 10.0 Out of Scope

- Slack App Directory submission (manual install URL for now)
- Multi-workspace per Slack team (1:1 mapping for v1)
- Slack Enterprise Grid support (team-level only)
- Uninstall webhook handler (bot removal detected passively)
- Billing integration from Slack (UseZombie billing is separate)
- Custom slash commands (bot responds to channel messages, not /commands)
