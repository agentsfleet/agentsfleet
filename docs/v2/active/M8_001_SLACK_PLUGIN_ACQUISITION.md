# M8_001: Slack Plugin Acquisition — "Add UseZombie to Slack"

**Prototype:** v0.9.0
**Milestone:** M8
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — Primary distribution channel; zero-friction onboarding
**Batch:** B4 — after M4 (approval gate), M6 (firewall)
**Branch:** feat/m8-slack-plugin
**Depends on:** M4_001 (approval gate), M3_001 (Slack tool)

---

## Overview

**Goal (testable):** A workspace owner clicks "Add UseZombie to Slack" from the dashboard.
OAuth completes. UseZombie automatically stores the Slack bot token in the zombie vault
(`vault.secrets(workspace_id, key_name="slack")`), creates a `workspace_integrations` row
for event routing, and posts a single confirmation message to Slack. The zombie vault is
UseZombie's own encrypted store (`vault.secrets` table via `crypto_store`) — not the
operator's 1Password/op CLI. Any zombie in that workspace can subsequently call
`credential_ref: "slack"` via M9's execute pipeline; the vault supplies the token without
the zombie ever seeing it.

**Alternate CLI path:** A workspace that bypasses OAuth can run `zombiectl credential add slack`
to store a bot token manually. This stores to the same vault slot and creates a
`workspace_integrations` row with `source='cli'`. Both paths converge at the same runtime
credential lookup.

**Not in scope for M8:** Grant authorization (M9), the execute proxy pipeline (M9), per-zombie
bot tokens (v3). M8 owns acquisition and credential bootstrap only.

---

## Credential and Vault Model

```
"Zombie vault" = vault.secrets table, encrypted via crypto_store.
NOT the operator's op/1Password. Two entirely separate systems.

Operator vault (op/1Password):  infra secrets, deploy keys, DB passwords.
Zombie vault (vault.secrets):   runtime credentials Zombies use to call external services.
```

**After M8 OAuth completes:**
- Bot token stored: `crypto_store.store(conn, workspace_id, "slack", token, version)`
- Routing record created: `core.workspace_integrations(provider="slack", external_id=team_id, source="oauth")`
- `workspace_integrations` has NO credential column — vault is the single source of truth

**At M9 runtime (zombie calling Slack):**
- Zombie calls `POST /v1/execute { credential_ref: "slack", ... }`
- Execute pipeline calls `crypto_store.load(conn, workspace_id, "slack")` → token injected
- `workspace_integrations` not consulted for credentials — only for routing

---

## 1.0 Slack App OAuth Flow

**Status:** PENDING

`GET /v1/slack/install` — redirects to `slack.com/oauth/v2/authorize` with:
- `client_id` from env `SLACK_CLIENT_ID`
- `scope`: `chat:write channels:read channels:history reactions:write users:read`
- `state`: HMAC-SHA256 signed JSON `{nonce, workspace_id?}`, base64url encoded
- `redirect_uri`: `{APP_URL}/v1/slack/callback`

State nonce stored in zombie vault Redis with 10-minute TTL.

`GET /v1/slack/callback?code=xxx&state=yyy`:
1. HMAC-validate state (no Redis roundtrip on tampered state) → `UZ-SLACK-001` on mismatch
2. Verify nonce exists in Redis → `UZ-SLACK-001` on miss (replay protection)
3. Exchange `code` for bot token via `POST https://slack.com/api/oauth.v2.access` → `UZ-SLACK-002` on failure
4. Parse `access_token`, `team.id`, `team.name`, `authed_user.id` from response
5. Create tenant + workspace if none exists (same pattern as `github_callback.zig`)
6. Store bot token: `crypto_store.store(conn, workspace_id, "slack", access_token, 1)`
7. Upsert `core.workspace_integrations` row (`provider="slack"`, `external_id=team_id`, `source="oauth"`)
8. Post confirmation message via `chat.postMessage` (direct use — bootstrap exception before M9 grant)
9. Redirect to `{APP_URL}/dashboard?slack=connected`

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleInstall`
  - input: `GET /v1/slack/install`
  - expected: `HTTP 302 to slack.com/oauth/v2/authorize with client_id, scope, state, redirect_uri`
  - test_type: unit
- 1.2 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `GET /v1/slack/callback?code=xxx&state=<valid_hmac_state>`
  - expected: `Token stored in vault.secrets("slack"), workspace_integrations row created, HTTP 302 to dashboard`
  - test_type: integration (DB + vault + HTTP mock for Slack API)
- 1.3 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `GET /v1/slack/callback with tampered state (CSRF)`
  - expected: `HTTP 403 UZ-SLACK-001, Slack API NOT called`
  - test_type: unit
- 1.4 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `Re-install: team already has a workspace_integrations row`
  - expected: `Existing row updated (no duplicate), vault token refreshed, HTTP 302`
  - test_type: integration (DB)

---

## 2.0 Confirmation Message (Bootstrap Exception)

**Status:** PENDING

Immediately after OAuth completes, the callback handler posts one message to the Slack
workspace's default channel using the just-acquired bot token. This is the only place in
M8 where we use the token directly — all subsequent Slack usage goes through M9's execute
pipeline + grant authorization.

Message is a constant — no Block Kit template builder, no LLM reasoning needed here:

```
const SLACK_CONNECTED_MSG =
    \\{"blocks":[{"type":"section","text":{"type":"mrkdwn",
    \\"text":"*UseZombie is connected!*\nYour zombie vault now has Slack credentials.\n
    \\Configure your Zombie at https://app.usezombie.com and request Slack access — your
    \\team will see approval requests here."}}]}
;
```

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `Successful OAuth callback`
  - expected: `chat.postMessage called once to Slack API with SLACK_CONNECTED_MSG`
  - test_type: integration (HTTP mock for Slack API)
- 2.2 PENDING
  - target: `src/http/handlers/slack_oauth.zig:handleCallback`
  - input: `Slack chat.postMessage fails (rate limit or channel error)`
  - expected: `Failure logged, HTTP 302 to dashboard still issued — token is stored, message failure is non-fatal`
  - test_type: unit

---

## 3.0 Workspace Integration Record

**Status:** PENDING

`core.workspace_integrations` is routing metadata only. It maps `(provider, external_id)`
to `workspace_id` so incoming Slack events can be routed to the right workspace.
No credentials live here — `vault.secrets` is the single source of truth.

Schema (`schema/028_workspace_integrations.sql`):
```sql
CREATE TABLE IF NOT EXISTS core.workspace_integrations (
    integration_id  UUID    PRIMARY KEY,
    workspace_id    UUID    NOT NULL REFERENCES core.workspaces(workspace_id),
    provider        TEXT    NOT NULL,
    external_id     TEXT    NOT NULL,   -- Slack team_id (T01ABC), Discord guild_id, etc.
    scopes_granted  TEXT    NOT NULL DEFAULT '',
    source          TEXT    NOT NULL DEFAULT 'oauth',  -- 'oauth' | 'cli'
    status          TEXT    NOT NULL DEFAULT 'active', -- 'active' | 'paused' | 'revoked'
    installed_at    BIGINT  NOT NULL,
    updated_at      BIGINT  NOT NULL,
    UNIQUE(provider, external_id)
);
```

**CLI path:** `zombiectl credential add slack <token>` stores the token in `vault.secrets`
and upserts a `workspace_integrations` row with `source='cli'`. Same data model, different
acquisition path.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `src/state/workspace_integrations.zig:upsertIntegration`
  - input: `provider="slack", external_id="T01ABC", workspace_id=<uuid>, no existing row`
  - expected: `New row created, integration_id returned, source="oauth"`
  - test_type: integration (DB)
- 3.2 PENDING
  - target: `src/state/workspace_integrations.zig:upsertIntegration`
  - input: `provider="slack", external_id="T01ABC", row already exists`
  - expected: `Existing row updated (no duplicate), updated_at refreshed`
  - test_type: integration (DB)
- 3.3 PENDING
  - target: `schema/028_workspace_integrations.sql`
  - input: `Migration applied to fresh DB`
  - expected: `Table created, UNIQUE(provider, external_id) enforced, no credential_ref column`
  - test_type: integration (DB)

---

## 4.0 Slack Event Routing

**Status:** PENDING

`POST /v1/slack/events` receives Slack Events API payloads. Routing:

```
incoming event
  → verify Slack signing secret (webhook_verify.SLACK, UZ-WH-010/011 on failure)
  → if type=url_verification: echo challenge (HTTP 200)
  → if event.bot_id present: ignore (prevent bot loops, HTTP 200)
  → lookup: SELECT workspace_id FROM core.workspace_integrations
            WHERE provider='slack' AND external_id=$1 AND status='active'
  → no row: ignore (HTTP 200, workspace hasn't installed UseZombie)
  → find active Zombie in workspace configured to handle Slack events
  → enqueue event to Redis worker queue
  → HTTP 200
```

All signing verification uses existing `webhook_verify.zig:verifySignature(SLACK, ...)`.
Constant-time comparison — RULE CTM enforced.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `Slack event_callback: team_id="T01ABC", valid signature`
  - expected: `workspace_id resolved via integration lookup → Zombie found → event enqueued`
  - test_type: integration (DB + Redis)
- 4.2 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `Slack event for team_id with no workspace_integrations row`
  - expected: `HTTP 200, nothing enqueued`
  - test_type: integration (DB)
- 4.3 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `Slack event with bot_id set`
  - expected: `HTTP 200, nothing enqueued (bot loop prevention)`
  - test_type: unit
- 4.4 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `Slack event with invalid signing secret`
  - expected: `HTTP 401 UZ-WH-010`
  - test_type: unit
- 4.5 PENDING
  - target: `src/http/handlers/slack_events.zig:handleSlackEvent`
  - input: `type=url_verification with valid signature`
  - expected: `HTTP 200 with {"challenge": "..."}`
  - test_type: unit

---

## 5.0 Slack Interactions (Approval Gate Relay)

**Status:** PENDING

`POST /v1/slack/interactions` receives Slack button click callbacks (e.g. Approve/Deny
from M4 approval gate messages). After signature verification, this delegates directly
to the existing `handleApprovalCallback` logic — no new approval gate code needed.

**Dimensions (test blueprints):**
- 5.1 PENDING
  - target: `src/http/handlers/slack_interactions.zig:handleInteraction`
  - input: `Slack interaction payload with valid signing secret, action_id matches approval gate`
  - expected: `Delegated to approval gate handler, HTTP 200`
  - test_type: integration
- 5.2 PENDING
  - target: `src/http/handlers/slack_interactions.zig:handleInteraction`
  - input: `Slack interaction with invalid signing secret`
  - expected: `HTTP 401 UZ-WH-010`
  - test_type: unit

---

## 6.0 Interfaces

**Status:** PENDING

### 6.1 API Endpoints

```
GET  /v1/slack/install        — redirect to Slack OAuth
GET  /v1/slack/callback       — OAuth exchange, vault store, routing record, confirmation msg
POST /v1/slack/events         — Slack Events API receiver → route to zombie
POST /v1/slack/interactions   — Slack button callbacks → relay to approval gate
```

### 6.2 New Zig Files

| File | Exports | Lines budget |
|------|---------|-------------|
| `src/state/workspace_integrations.zig` | `upsertIntegration` | ≤ 150 |
| `src/http/handlers/slack_oauth.zig` | `handleInstall`, `handleCallback` | ≤ 350 |
| `src/http/handlers/slack_events.zig` | `handleSlackEvent` | ≤ 350 |
| `src/http/handlers/slack_interactions.zig` | `handleInteraction` | ≤ 350 |

### 6.3 Error Contracts

| Condition | Code | HTTP |
|-----------|------|------|
| OAuth state mismatch / replay | `UZ-SLACK-001` | 403 |
| Slack token exchange failed | `UZ-SLACK-002` | 502 |
| Bot token expired (future use) | `UZ-SLACK-003` | 401 |
| Invalid Slack signing secret | `UZ-WH-010` (existing) | 401 |
| Stale Slack timestamp | `UZ-WH-011` (existing) | 401 |

---

## 7.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | Behavior | User sees |
|---------|---------|----------|-----------|
| OAuth state tampered | CSRF attempt | HTTP 403, token exchange NOT called | Error page |
| Slack token exchange fails | Network / secret wrong | HTTP 502, no vault write | Error page, retry link |
| Confirmation message fails | Rate limit / channel | Logged, non-fatal — token IS stored | Nothing (silent) |
| Re-install (same team) | User clicks install again | Token refreshed in vault, row updated, no duplicate | Connected again |
| Bot removed from Slack | Admin uninstalls | Events stop arriving; row status stays active until token fails | Zombie stops receiving events |

---

## 8.0 Implementation Constraints

**Status:** PENDING

| Constraint | Verify |
|-----------|--------|
| Bot token stored in `vault.secrets` only — never in `workspace_integrations` | `grep credential_ref schema/028*` returns zero hits |
| OAuth state uses HMAC-SHA256 | Code review |
| Signing secret verified constant-time on all POST endpoints | RULE CTM, tests 4.4, 5.2 |
| `workspace_integrations` has no `credential_ref` column | Schema inspection |
| CLI and OAuth paths converge at same `vault.secrets` slot | Code review |
| Every file ≤ 350 lines | RULE FLL |
| Cross-compile both targets | RULE XCC |
| Schema migration ≤ 100 lines | `wc -l schema/028*` |

---

## 9.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | `src/types/id_format.zig` — add `generateIntegrationId` | `zig build` compiles |
| 2 | `src/state/workspace_integrations.zig` — `upsertIntegration` + tests | `make test` passes |
| 3 | `src/http/handlers/slack_oauth.zig` — `handleInstall` + `handleCallback` | Tests 1.1–1.4, 2.1–2.2 pass |
| 4 | `src/http/handlers/slack_events.zig` — `handleSlackEvent` | Tests 4.1–4.5 pass |
| 5 | `src/http/handlers/slack_interactions.zig` — `handleInteraction` | Tests 5.1–5.2 pass |
| 6 | Router, handler.zig, server.zig — wire 4 routes | `zig build` compiles |
| 7 | `main.zig` — add new files to test discovery | `make test` passes |
| 8 | Full gate: `make test && make test-integration && make lint` | All green |
| 9 | Cross-compile: x86_64-linux + aarch64-linux | RULE XCC |

---

## 10.0 Acceptance Criteria

**Status:** PENDING

- [ ] OAuth completes: token in `vault.secrets(workspace_id, "slack")`, routing row created
- [ ] Re-install refreshes vault token and updates row (no duplicate row)
- [ ] Confirmation message posted to Slack after OAuth (non-fatal if it fails)
- [ ] CSRF state tamper returns 403 without calling Slack API
- [ ] Slack events routed to correct workspace via `workspace_integrations` lookup
- [ ] Bot messages filtered (no loops)
- [ ] Slack interactions relayed to approval gate
- [ ] `workspace_integrations` has no `credential_ref` column
- [ ] `make test && make lint` pass
- [ ] Cross-compile passes (RULE XCC)

---

## Applicable Rules

RULE XCC, RULE FLL, RULE ORP, RULE FLS, RULE CTM, RULE NSQ.

---

## Invariants

- `vault.secrets` is the single source of truth for the Slack bot token.
- `core.workspace_integrations` contains routing metadata only — never credentials.
- `workspace_integrations.credential_ref` column does not exist — if it appears in any diff, stop and fix.
- CLI path (`zombiectl credential add slack`) and OAuth path both write to `vault.secrets(workspace_id, "slack")`.
- M8 posts to Slack directly exactly once (bootstrap confirmation). All other Slack usage routes through M9 execute pipeline.

---

## Eval Commands

```bash
# E1: Build
zig build 2>&1 | head -5; echo "build=$?"

# E2: Tests
make test 2>&1 | tail -5; echo "test=$?"

# E3: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E4: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E6: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "xc_x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "xc_arm=$?"

# E7: No credential_ref in workspace_integrations schema
grep credential_ref schema/028_workspace_integrations.sql; echo "cred_ref_check=$?"

# E8: No raw token in DB code
grep -rn "bot_token\|access_token" src/ --include="*.zig" | grep -v "vault\|crypto_store\|test\|comment"
```

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| 350L gate | see E5 | | |
| drain check | `make check-pg-drain` | | |
| Gitleaks | `gitleaks detect` | | |
| No credential_ref in schema | see E7 | | |

---

## Out of Scope

- Grant authorization for zombie Slack usage (M9)
- Execute proxy pipeline (M9)
- Per-zombie bot tokens (v3)
- Slack App Directory submission
- Multi-workspace per Slack team (1:1 for v2)
- Slack Enterprise Grid
- Uninstall webhook handler
- Block Kit template builders (LLM-driven Zombie handles output formatting)
- Slack modals for credential collection (token acquired via OAuth, not UI forms)
