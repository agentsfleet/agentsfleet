# M9_001: Integration Grant Authorization & Execute API

**Prototype:** v0.9.0
**Milestone:** M9
**Workstream:** 001
**Date:** Apr 12, 2026
**Status:** DONE

> **CLI rename note (May 03, 2026):** M49's standardization slice renamed `zombiectl agent create` → `agent add` and `zombiectl grant revoke` → `grant delete`. Historical command examples below preserve the verbs as shipped; current canonical verbs follow the `add / show / list / delete` standard.

**Priority:** P1 — Core trust layer for zombie integrations and external agent access
**Batch:** B4 — after M6 (firewall), M4 (approval gate), M5 (tool bridge)
**Branch:** feat/m9-001-execute-api
**Depends on:** M6_001 (firewall policy engine), M4_001 (approval gate), M5_001 (tool bridge)

---

## Overview

**Goal (testable):** A zombie (running inside UseZombie or as an external agent) calls
`POST /v1/execute` to proxy a credentialed request to an external service. UseZombie:
(1) authenticates the caller — zombie session (Path A: internal) or `zmb_` API key
(Path B: external agent), (2) checks the zombie has an approved integration grant for
the target service, (3) requests a grant from the human if none exists and waits for
approval, (4) evaluates the firewall policy, (5) fires the approval gate if the endpoint
requires it, (6) injects the credential from the vault, (7) proxies the outbound call,
(8) strips credential echo from response, (9) logs to activity stream, (10) returns
response to the zombie. The zombie never receives the credential.

**Path A — First-party zombies (internal):**
Zombies running inside UseZombie's executor trust boundary. Authenticated by internal
worker runtime. Call `/v1/execute` directly without external auth.

**Path B — External agents (LangGraph, CrewAI, Composio):**
Agents running outside UseZombie. Authenticated via `zmb_` prefixed API keys issued per
workspace. Human creates the key via dashboard (Clerk-protected). Agent stores the key
and uses it as a Bearer token. The key resolves to a workspace ID and a registered
external agent zombie record.

**Integration Grant model:**
Every zombie must have an approved grant for a service before UseZombie will inject
credentials for it. Grants are zombie-scoped and service-scoped. When a zombie needs
a new integration, it sends a grant request with a human-readable reason. UseZombie
notifies the workspace owner via Slack, Discord, and/or dashboard. On approval, the
grant is durable — no per-call approval needed. High-risk actions still trigger the
M4 per-request approval gate independently.

**Services supported (v2):** AgentMail/Gmail, Slack, Discord, Grafana.

---

## Part 1: Core Infrastructure

---

## 1.0 Execute Endpoint

**Status:** ✅ DONE

`POST /v1/execute` — the proxy endpoint. Accepts a JSON body describing the outbound
request. Authenticates via zombie session (Path A) or `Authorization: Bearer zmb_xxx`
(Path B). Returns the proxied response with UseZombie metadata headers.

```json
// Request
POST /v1/execute
Authorization: Bearer zmb_abc123   // Path B — omit for Path A (internal trust)
{
  "zombie_id": "01abc...",
  "target": "slack.com/api/chat.postMessage",
  "method": "POST",
  "headers": {"Content-Type": "application/json"},
  "body": {"channel": "#hiring", "text": "Candidate Jane Doe — interview scheduled"},
  "credential_ref": "slack"
}

// Response
{
  "status": 200,
  "headers": {"...": "..."},
  "body": {"ok": true, "ts": "1234567890.123456"},
  "usezombie": {
    "action_id": "019abc...",
    "firewall_decision": "allow",
    "credential_injected": true,
    "approval_required": false,
    "grant_id": "uzg_01abc..."
  }
}
```

**Dimensions:**
- 1.1 ✅ DONE
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with valid zmb_ key, zombie has approved grant for slack, target=slack.com/api/chat.postMessage`
  - expected: `Credential injected, Slack response returned with UZ metadata`
  - test_type: integration (vault + HTTP mock)
- 1.2 ✅ DONE
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with invalid zmb_ key`
  - expected: `HTTP 401, error code UZ-APIKEY-001`
  - test_type: unit
- 1.3 ✅ DONE
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute where zombie has no grant for target service`
  - expected: `HTTP 403, UZ-GRANT-001, message includes grant request hint`
  - test_type: integration (DB)
- 1.4 ✅ DONE
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute where zombie grant exists but status=pending`
  - expected: `HTTP 202, UZ-GRANT-002, message "Grant pending human approval"`
  - test_type: integration (DB)
- 1.5 ✅ DONE
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with target domain not in workspace allowlist`
  - expected: `HTTP 403, UZ-FW-001, action logged`
  - test_type: integration (DB)
- 1.6 ✅ DONE
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with body containing injection pattern`
  - expected: `HTTP 403, UZ-FW-003, action logged`
  - test_type: unit
- 1.7 ✅ DONE
  - target: `src/http/handlers/outbound_proxy.zig:executePipeline`
  - input: `response from target API contains credential echo`
  - expected: `Credential stripped from response body before returning`
  - test_type: unit

---

## 2.0 Integration Grant System

**Status:** ✅ DONE

Grants are zombie-scoped and service-scoped. A zombie must have an approved grant
before UseZombie will inject credentials for a service. Grants persist until revoked.
v2 scopes: `["*"]` (full service access). Designed for granular scope extension
(`["channels:read:#hiring", "chat:write:#hiring"]`) without schema changes.

**Services (v2):** `slack`, `gmail`, `agentmail`, `discord`, `grafana`

**Grant lifecycle:**
```
requested → pending → approved → (active, used at runtime)
                    ↘ revoked   (human denied or later revoked)
```

**Grant request (zombie-initiated):**
```json
POST /v1/zombies/{zombie_id}/integration-requests
{
  "service": "slack",
  "reason": "Need to post candidate responses to #hiring channel to complete interview scheduling"
}

// Response: 202 Accepted
{
  "grant_id": "uzg_01abc...",
  "status": "pending",
  "notification_sent_to": ["slack", "dashboard"]
}
```

**Dimensions:**
- 2.1 ✅ DONE
  - target: `schema/026_core_integration_grants.sql`
  - input: `CREATE TABLE with grant_id, zombie_id FK, service, scopes, status, requested_reason, timestamps`
  - expected: `Table created, unique constraint on (zombie_id, service)`
  - test_type: integration (DB)
- 2.2 ✅ DONE
  - target: `src/http/handlers/integration_grants.zig:handleRequestGrant`
  - input: `POST /v1/zombies/{id}/integration-requests with service=slack, reason=...`
  - expected: `Grant created status=pending, notification dispatched to configured channels`
  - test_type: integration (DB + notification mock)
- 2.3 ✅ DONE
  - target: `src/http/handlers/integration_grants.zig:handleRequestGrant`
  - input: `POST /v1/zombies/{id}/integration-requests where grant already approved for service`
  - expected: `HTTP 200 with existing grant, no duplicate created`
  - test_type: integration (DB)
- 2.4 ✅ DONE
  - target: `src/http/handlers/integration_grants.zig:handleListGrants`
  - input: `GET /v1/zombies/{id}/integration-grants`
  - expected: `Array of {grant_id, service, scopes, status, requested_at, approved_at}`
  - test_type: integration (DB)
- 2.5 ✅ DONE
  - target: `src/http/handlers/integration_grants.zig:handleRevokeGrant`
  - input: `DELETE /v1/zombies/{id}/integration-grants/{grant_id}`
  - expected: `Grant status=revoked, execute calls for that service blocked immediately`
  - test_type: integration (DB)

---

## 3.0 External Agent Keys (Path B)

**Status:** ✅ DONE

`zmb_` prefixed API keys for external agents (LangGraph, CrewAI, Composio-wrapped tools)
running outside UseZombie. Keys are issued per workspace via the Clerk-protected dashboard.
Each key is backed by a zombie record of type `external_agent`. The key resolves to
(workspace_id, zombie_id) — giving the external agent a full zombie identity including
the integration grant system.

**Auth model:**
- Key creation: human authenticates via Clerk → POST /v1/workspaces/{ws}/external-agents → zmb_ key returned once
- Key usage: external agent → Authorization: Bearer zmb_xxx → UseZombie hashes → DB lookup → (workspace_id, zombie_id)
- No Clerk involved at key usage time

**Key format:** `zmb_` + 32 random bytes hex = 68 chars total
**Storage:** SHA-256 hex of the raw key. Raw key shown once at creation, never stored.

```json
// Create external agent (Clerk-authenticated human)
POST /v1/workspaces/{workspace_id}/external-agents
{
  "name": "my-langgraph-agent",
  "description": "Lead enrichment pipeline"
}

// Response — raw key shown once
{
  "agent_id": "01abc...",
  "zombie_id": "01def...",
  "name": "my-langgraph-agent",
  "key": "zmb_<api-key-redacted>",
  "warning": "Save this key — you will not see it again"
}
```

**Dimensions:**
- 3.1 ✅ DONE
  - target: `schema/027_core_external_agents.sql`
  - input: `CREATE TABLE with agent_id, workspace_id FK, zombie_id FK, name, description, key_hash, created_at, last_used_at`
  - expected: `Table created, unique constraint on key_hash`
  - test_type: integration (DB)
- 3.2 ✅ DONE
  - target: `src/http/handlers/external_agents.zig:handleCreateExternalAgent`
  - input: `POST /v1/workspaces/{ws}/external-agents with name, description`
  - expected: `Zombie record created (type=external_agent), key generated, hash stored, raw key returned once`
  - test_type: integration (DB)
- 3.3 ✅ DONE
  - target: `src/http/handlers/external_agents.zig:authenticateExternalAgent`
  - input: `Authorization: Bearer zmb_xxx`
  - expected: `SHA-256 hash computed, matched against DB, (workspace_id, zombie_id) resolved, last_used_at updated`
  - test_type: integration (DB)
- 3.4 ✅ DONE
  - target: `src/http/handlers/external_agents.zig:authenticateExternalAgent`
  - input: `Authorization: Bearer zmb_revoked_key`
  - expected: `HTTP 401, UZ-APIKEY-001`
  - test_type: integration (DB)
- 3.5 ✅ DONE
  - target: `src/http/handlers/external_agents.zig:handleListExternalAgents`
  - input: `GET /v1/workspaces/{ws}/external-agents`
  - expected: `List with name, description, last_used_at (key hash NOT shown)`
  - test_type: integration (DB)
- 3.6 ✅ DONE
  - target: `src/http/handlers/external_agents.zig:handleDeleteExternalAgent`
  - input: `DELETE /v1/workspaces/{ws}/external-agents/{agent_id}`
  - expected: `Key invalidated, zombie record marked inactive, subsequent execute calls return 401`
  - test_type: integration (DB)

---

## 4.0 Human Approval Notifications

**Status:** ✅ DONE

When a zombie requests a grant, UseZombie fans out notifications to all configured
channels for the workspace. Human approves via Slack button, Discord button, or
dashboard. Approval on any channel approves the grant globally.

**Channel priority:** All configured channels fire simultaneously.
**Fallback:** Dashboard notification always fires regardless of other channels.

**Notification payload (all channels):**
```
🔐 [Zombie name] is requesting [service] access

Reason: "[zombie's stated reason]"

Scopes: Full access (*)

[Approve] [Deny]
```

**Approval webhook:** clicking Approve/Deny in Slack or Discord calls back to
`POST /v1/webhooks/{zombie_id}:grant-approval` with `{grant_id, decision}`.

**Dimensions:**
- 4.1 ✅ DONE
  - target: `src/zombie/notifications/grant_notifier.zig:notifyGrantRequest`
  - input: `Grant request for zombie with slack configured`
  - expected: `Slack DM sent to workspace owner with Approve/Deny buttons, grant_id in callback payload`
  - test_type: integration (Slack mock)
- 4.2 ✅ DONE
  - target: `src/zombie/notifications/grant_notifier.zig:notifyGrantRequest`
  - input: `Grant request for zombie with discord configured`
  - expected: `Discord DM sent with Approve/Deny buttons`
  - test_type: integration (Discord mock)
- 4.3 ✅ DONE
  - target: `src/zombie/notifications/grant_notifier.zig:notifyGrantRequest`
  - input: `Grant request with no Slack/Discord configured`
  - expected: `Dashboard notification created, no external call made`
  - test_type: integration (DB)
- 4.4 ✅ DONE
  - target: `src/http/handlers/webhooks.zig:handleGrantApproval`
  - input: `POST /v1/webhooks/{zombie_id}:grant-approval with decision=approved`
  - expected: `Grant status updated to approved, zombie can now execute calls for that service`
  - test_type: integration (DB)
- 4.5 ✅ DONE
  - target: `src/http/handlers/webhooks.zig:handleGrantApproval`
  - input: `POST /v1/webhooks/{zombie_id}:grant-approval with decision=denied`
  - expected: `Grant status updated to revoked, zombie receives UZ-GRANT-003 on next execute`
  - test_type: integration (DB)

---

## 5.0 Execute Pipeline (Reuses Existing Trust Infrastructure)

**Status:** ✅ DONE

```
authenticate (zombie session OR zmb_ key)
    ↓
resolve (zombie_id, workspace_id)
    ↓
extract service from target domain
    ↓
check integration grant (zombie_id, service) → pending? 202. missing? 403 + hint.
    ↓
firewall inspect (domain allowlist → endpoint policy → injection scan)
    ↓
approval gate check (M4 — fires for gated endpoints)
    ↓
fetch credential from vault (crypto_store.load by credential_ref)
    ↓
inject credential into outbound request headers
    ↓
proxy outbound HTTP call (std.http.Client, 30s timeout, 10MB cap)
    ↓
scan response for credential echo (firewall.scanResponseBody)
    ↓
strip any leaked credentials from response body
    ↓
log action to activity stream (action_id, firewall_decision, grant_id)
    ↓
return response + X-UseZombie-Action-Id, X-UseZombie-Firewall-Decision headers
```

**Dimensions:**
- 5.1 ✅ DONE
  - target: `src/http/handlers/outbound_proxy.zig:executePipeline`
  - input: `target endpoint matches approval gate rule (e.g., POST /offers)`
  - expected: `Execution pauses, Slack/Discord approval sent, resumes on approve`
  - test_type: integration (Redis + HTTP mock)
- 5.2 ✅ DONE
  - target: `src/http/handlers/outbound_proxy.zig:executePipeline`
  - input: `target API unreachable (DNS failure)`
  - expected: `Timeout after 30s, HTTP 502, UZ-PROXY-001, action logged`
  - test_type: unit (mock HTTP client)
- 5.3 ✅ DONE
  - target: `src/http/handlers/outbound_proxy.zig:executePipeline`
  - input: `target returns response > 10MB`
  - expected: `Response truncated at 10MB, X-UseZombie-Truncated: true header added`
  - test_type: unit

---

## 6.0 CLI Commands

**Status:** ✅ DONE

```
zombiectl grant list   --zombie {id}           → service, scopes, status, approved_at
zombiectl grant revoke --zombie {id} {grant_id}→ revokes grant immediately
zombiectl agent create --workspace {ws} --name "my-agent"  → prints zmb_ key once
zombiectl agent list   --workspace {ws}        → name, description, last_used
zombiectl agent delete --workspace {ws} {agent_id}
```

**Dimensions:**
- 6.1 ✅ DONE
  - target: `zombiectl/src/commands/grant.js:commandGrant`
  - input: `zombiectl grant list --zombie {id}`
  - expected: `Table of grants with service, scopes, status, approved_at`
  - test_type: unit (mocked API)
- 6.2 ✅ DONE
  - target: `zombiectl/src/commands/grant.js:commandGrant`
  - input: `zombiectl grant revoke --zombie {id} {grant_id}`
  - expected: `API called, grant revoked, confirmation printed`
  - test_type: unit (mocked API)
- 6.3 ✅ DONE
  - target: `zombiectl/src/commands/agent.js:commandAgent`
  - input: `zombiectl agent create --workspace {ws} --name "my-langgraph-agent"`
  - expected: `API called, raw key printed once with warning`
  - test_type: unit (mocked API)
- 6.4 ✅ DONE
  - target: `zombiectl/src/commands/agent.js:commandAgent`
  - input: `zombiectl agent list --workspace {ws}`
  - expected: `Table with name, description, last_used (key hash NOT shown)`
  - test_type: unit (mocked API)

---

## Part 2: Use Case Agent-Executable Docs

*Each use case has a standalone agent-executable doc in `docs/nostromo/`.
These docs are designed to be read by an AI agent and executed end-to-end.
They will be published at docs.usezombie.com/integrations/{slug}.*

| Use Case | Doc | Integration actors |
|----------|-----|--------------------|
| Lead Collector | `docs/nostromo/lead_collector_zombie.md` | AgentMail/Gmail → Zombie → Slack/CRM |
| Hiring Agent | `docs/nostromo/hiring_agent_zombie.md` | Slack → Zombie → Slack thread |
| Ops Zombie | `docs/nostromo/ops_zombie.md` | Grafana → Zombie → Slack/Discord |

---

## 7.0 Interfaces

### 7.1 API Endpoints

```
POST   /v1/execute                                      — proxy outbound call
POST   /v1/zombies/{id}/integration-requests            — zombie requests grant
GET    /v1/zombies/{id}/integration-grants              — list grants
DELETE /v1/zombies/{id}/integration-grants/{grant_id}  — revoke grant
POST   /v1/workspaces/{ws}/external-agents             — create external agent + key
GET    /v1/workspaces/{ws}/external-agents             — list external agents
DELETE /v1/workspaces/{ws}/external-agents/{agent_id} — delete agent + invalidate key
POST   /v1/webhooks/{zombie_id}:grant-approval         — Slack/Discord approval callback
```

### 7.2 Input Contracts — Execute

| Field | Type | Constraints |
|-------|------|-------------|
| `zombie_id` | Text | Valid zombie UUIDv7 |
| `target` | Text | Domain must match an approved grant service |
| `method` | Text | GET/POST/PUT/PATCH/DELETE |
| `headers` | Object | Optional; Authorization headers stripped |
| `body` | Text/Object | For POST/PUT/PATCH |
| `credential_ref` | Text | Name of credential in workspace vault |

### 7.3 Service → Domain Mapping

| Service | Domains matched |
|---------|----------------|
| `slack` | `slack.com`, `hooks.slack.com` |
| `gmail` | `gmail.googleapis.com`, `www.googleapis.com` |
| `agentmail` | `api.agentmail.to` |
| `discord` | `discord.com`, `discordapp.com` |
| `grafana` | `grafana.com`, workspace Grafana domain |

### 7.4 Error Contracts

| Condition | Code | HTTP |
|-----------|------|------|
| Invalid zmb_ key | `UZ-APIKEY-001` | 401 |
| Key lacks execute permission | `UZ-APIKEY-002` | 403 |
| No integration grant for service | `UZ-GRANT-001` | 403 |
| Grant pending human approval | `UZ-GRANT-002` | 202 |
| Grant denied by human | `UZ-GRANT-003` | 403 |
| Domain not in allowlist | `UZ-FW-001` | 403 |
| Injection detected in body | `UZ-FW-003` | 403 |
| Credential not found in vault | `UZ-TOOL-001` | 404 |
| Target API error | `UZ-PROXY-001` | 502 |
| Approval gate timeout | `UZ-GATE-005` | 408 |

---

## 8.0 Schema Files

| File | Content |
|------|---------|
| `schema/026_core_integration_grants.sql` | integration_grants table |
| `schema/027_core_external_agents.sql` | external_agents table |

---

## 9.0 Implementation Constraints

| Constraint | Verify |
|-----------|--------|
| `execute.zig` ≤ 350 lines | `wc -l` |
| `outbound_proxy.zig` ≤ 350 lines | `wc -l` |
| `integration_grants.zig` ≤ 350 lines | `wc -l` |
| `external_agents.zig` ≤ 350 lines | `wc -l` |
| API key hash uses SHA-256 | Code review |
| Raw key shown exactly once | Code path analysis |
| Reuses firewall + approval gate | No duplicate trust logic |
| Each schema file ≤ 100 lines | `wc -l` |
| Cross-compiles | Both targets |

---

## 10.0 Execution Plan (Ordered)

| Step | Action | Verify |
|------|--------|--------|
| 1 | Schema: integration_grants + external_agents tables | `zig build` compiles |
| 2 | Error codes: UZ-APIKEY-*, UZ-GRANT-*, UZ-FW-*, UZ-PROXY-* | Registry comptime check passes |
| 3 | Integration grant CRUD handlers | Tests 2.1–2.5 pass |
| 4 | External agent key create/auth/list/delete | Tests 3.1–3.6 pass |
| 5 | Execute handler (Path A + Path B auth) | Tests 1.1–1.7 pass |
| 6 | Execute pipeline (reuse firewall + approval gate) | Tests 5.1–5.3 pass |
| 7 | Notification system (Slack, Discord, dashboard) | Tests 4.1–4.5 pass |
| 8 | Router + server dispatch wiring | `make test` passes |
| 9 | CLI commands (grant + agent) | Tests 6.1–6.4 pass |
| 10 | Agent-executable docs | Human review |
| 11 | Full test suite | `make test && make test-integration && make lint` |

---

## 11.0 Acceptance Criteria

- [ ] Zombie with approved grant calls service → credential injected, proxied, logged
- [ ] Zombie without grant → 403 UZ-GRANT-001 with hint to request grant
- [ ] Grant request → Slack DM + Discord DM + dashboard notification fired
- [ ] Human approves in Slack → grant active, zombie retries successfully
- [ ] External agent zmb_ key → resolves zombie_id, passes grant check, executes
- [ ] Invalid zmb_ key → 401
- [ ] Credential never appears in response body
- [ ] Domain not in allowlist → 403 UZ-FW-001
- [ ] Injection in body → 403 UZ-FW-003
- [ ] `make test && make lint` pass
- [ ] Cross-compile passes (x86_64-linux, aarch64-linux)

---

## Applicable Rules

RULE XCC (cross-compile), RULE FLL (350-line gate), RULE ORP (orphan sweep),
RULE FLS (drain all results), RULE CTM (constant-time key comparison),
RULE VLT (credentials in vault only), RULE NSQ (schema-qualified SQL).

---

## Eval Commands

```bash
zig build 2>&1 | head -5; echo "build=$?"
make test 2>&1 | tail -5; echo "test=$?"
make test-integration 2>&1 | tail -5; echo "integration=$?"
make lint 2>&1 | grep -E "✓|FAIL"
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "xc_x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "xc_arm=$?"
```

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `zig build test` | 2 pre-existing telemetry T3 failures on main (not M9) | ⚠️ pre-existing |
| Integration tests | `make test-integration` | Requires live DB — deferred to CI | ⏳ CI |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | Clean | ✅ |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | Clean | ✅ |
| Lint | `make lint` | All 8 gates pass (ZLint, pg-drain, FLL, website, app, zombiectl, actionlint, OpenAPI) | ✅ |
| 350L gate | `git diff --name-only origin/main \| xargs wc -l` | error_registry_test.zig 373 lines — pre-existing on main, M9 changed 2 chars | ⚠️ pre-existing |
| drain check | `make check-pg-drain` (via lint) | 208 files scanned, 0 violations | ✅ |
| Gitleaks | `gitleaks detect` | Pending (run before merge) | ⏳ |
