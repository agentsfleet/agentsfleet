# M1_001: Lead Zombie — 2-command hero: install, up, agent is live

**Prototype:** v0.5.0
**Milestone:** M1
**Workstream:** 001
**Date:** Apr 08, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — First v2 Zombie, proves core architecture
**Batch:** B1 — no dependencies
**Branch:** feat/m1-lead-zombie
**Depends on:** None (v1 executor, vault, and worker fleet are already shipped)

---

## Overview

**Goal (testable):** A developer runs `zombiectl install lead-collector && zombiectl up` and within 2 minutes has a live Zombie that receives inbound email webhooks, processes them via a NullClaw agent in the bwrap+landlock sandbox, replies via the agentmail API with credentials injected from the vault, and logs every action to the activity stream. A sample workspace ships pre-configured so the hero demo works with zero credential setup.

**Problem:** v2 has no working Zombie and no developer-facing CLI for Zombies. The architecture (Zombie config, event loop, webhook routing, activity stream, session checkpoint) exists only as design decisions in the eng review. Nothing runs. The v1 CLI speaks "specs" and "harness," not "install" and "up." A YC founder or indie hacker with a working agent wants 2 commands and a running Zombie, not a config tutorial.

**Solution summary:** Build four new modules in `src/zombie/` (config parser, event loop, activity stream, webhook handler), three new database tables (`core.zombies`, `core.zombie_sessions`, `core.activity_events`), and minimal CLI commands in `zombiectl/` (`install`, `up`, `status`, `kill`, `logs`, `credential add/list`). A bundled lead-collector template ships inside the npm package. A pre-configured sample workspace enables the hero demo with zero credential setup. `zombiectl up` deploys to UseZombie cloud (remote-first, no local bwrap needed). The event loop evolves the v1 worker's gate loop pattern into a persistent Zombie runner. The executor, vault, and billing systems are reused without modification.

---

## 1.0 Zombie Configuration Format

**Status:** PENDING

Define the TOML-based configuration format that describes a Zombie: its name, tools, trigger, policy, and credential references. The config is stored in the `core.zombies` table as parsed JSONB and loaded by the worker at Zombie claim time.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/zombie/config.zig:parseZombieConfig`
  - input: `valid TOML string with name, trigger (type=webhook, source=agentmail), tools=[agentmail], credentials=[op://ZMB_LOCAL_DEV/agentmail/api_key]`
  - expected: `ZombieConfig struct with all fields populated, no error`
  - test_type: unit
- 1.2 PENDING
  - target: `src/zombie/config.zig:parseZombieConfig`
  - input: `TOML string missing required "name" field`
  - expected: `error.MissingRequiredField with field name in error detail`
  - test_type: unit
- 1.3 PENDING
  - target: `src/zombie/config.zig:validateToolAttachments`
  - input: `ZombieConfig with tools=["unknown_tool"]`
  - expected: `error.UnknownTool with tool name in error detail`
  - test_type: unit
- 1.4 PENDING
  - target: `src/zombie/config.zig:parseZombieConfig`
  - input: `TOML string with trigger.type="invalid_type"`
  - expected: `error.InvalidTriggerType`
  - test_type: unit

---

## 2.0 Webhook Endpoint

**Status:** PENDING

New HTTP route that receives webhooks from external services (agentmail for Phase 1), validates the payload, looks up the Zombie registered for that source, and enqueues the event on the Zombie's Redis Streams queue. Includes idempotency dedup via event ID.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/http/handlers/webhooks.zig:receiveWebhook`
  - input: `POST /webhooks/agentmail with valid JSON payload {event_id, type: "message.received", data: {from, subject, body}}`
  - expected: `HTTP 202 Accepted, event enqueued on zombie:{zombie_id}:events stream`
  - test_type: integration (Redis)
- 2.2 PENDING
  - target: `src/http/handlers/webhooks.zig:receiveWebhook`
  - input: `POST /webhooks/agentmail with duplicate event_id (already processed)`
  - expected: `HTTP 200 OK (idempotent), event NOT re-enqueued`
  - test_type: integration (Redis)
- 2.3 PENDING
  - target: `src/http/handlers/webhooks.zig:receiveWebhook`
  - input: `POST /webhooks/agentmail with no Zombie registered for agentmail source`
  - expected: `HTTP 404, error code UZ-WH-001`
  - test_type: integration (DB)
- 2.4 PENDING
  - target: `src/http/handlers/webhooks.zig:receiveWebhook`
  - input: `POST /webhooks/agentmail with malformed JSON`
  - expected: `HTTP 400, error code UZ-WH-002, Zombie NOT affected`
  - test_type: unit

---

## 3.0 Zombie Event Loop

**Status:** PENDING

The persistent agent process that claims a Zombie, loads its config and session checkpoint, waits for events on the Redis Streams queue, delivers each event to the NullClaw agent running in the executor sandbox, checkpoints state after each event, and logs to the activity stream. Sequential mailbox ordering. At-least-once delivery (Redis consumer group ack after processing). Imports budget checks, kill switch, and interrupt handling from v1 shared primitives.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `src/zombie/event_loop.zig:claimZombie`
  - input: `zombie_id for an unclaimed Zombie`
  - expected: `Zombie claimed, config loaded, session checkpoint loaded (or fresh start), agent initialized in executor`
  - test_type: integration (DB + Redis + Executor)
- 3.2 PENDING
  - target: `src/zombie/event_loop.zig:deliverEvent`
  - input: `webhook event on queue, agent running in sandbox`
  - expected: `Agent processes event, response captured, state checkpointed to Postgres, activity event logged, Redis ack committed`
  - test_type: integration (DB + Redis + Executor)
- 3.3 PENDING
  - target: `src/zombie/event_loop.zig:crashRecovery`
  - input: `Zombie with existing checkpoint in core.zombie_sessions`
  - expected: `Agent reloaded with conversation context from checkpoint, resumes event processing`
  - test_type: integration (DB + Executor)
- 3.4 PENDING
  - target: `src/zombie/event_loop.zig:deliverEvent`
  - input: `duplicate event (same event_id delivered twice due to crash before ack)`
  - expected: `Agent handles gracefully (idempotent processing), no duplicate side effects`
  - test_type: integration (DB + Redis)

---

## 4.0 Activity Stream

**Status:** PENDING

Append-only event log per workspace. Every Zombie action (event received, agent response, tool call, error, checkpoint) is logged as an activity event. Writes are async and non-blocking — a write failure must never crash the Zombie or block event processing. Query support for listing by zombie_id or workspace_id with cursor-based pagination.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/zombie/activity_stream.zig:logEvent`
  - input: `ActivityEvent{zombie_id, workspace_id, event_type: "webhook_received", detail: {from, subject}, timestamp}`
  - expected: `Row inserted in core.activity_events, returns without error`
  - test_type: integration (DB)
- 4.2 PENDING
  - target: `src/zombie/activity_stream.zig:logEvent`
  - input: `ActivityEvent with simulated DB write failure (connection closed)`
  - expected: `Error logged internally, function returns without crashing, Zombie continues processing`
  - test_type: integration (DB)
- 4.3 PENDING
  - target: `src/zombie/activity_stream.zig:queryByZombie`
  - input: `zombie_id, cursor=null, limit=20`
  - expected: `First 20 events for this Zombie, ordered by created_at DESC, next_cursor returned`
  - test_type: integration (DB)
- 4.4 PENDING
  - target: `src/zombie/activity_stream.zig:queryByWorkspace`
  - input: `workspace_id, cursor=null, limit=20`
  - expected: `First 20 events across all Zombies in workspace, ordered by created_at DESC`
  - test_type: integration (DB)

---

## 5.0 CLI Commands + Zombie Templates

**Status:** PENDING

Minimal CLI commands for the v2 Zombie workflow. Flat top-level for common ops (`install`, `up`, `status`, `kill`, `logs`), namespaced for less common (`credential add`, `credential list`, `activity list`). A bundled lead-collector template ships inside the npm package. A pre-configured sample workspace allows the hero demo to work with zero credential setup. `zombiectl up` deploys to UseZombie cloud (remote-first).

```
Hero flow:
  $ zombiectl install lead-collector
    Lead Collector installed with sample workspace.

  $ zombiectl up
    lead-collector is live (demo mode).
    Send a test email to demo@mail.usezombie.com to see it work.

    To use your own credentials:
    zombiectl credential add agentmail

Real usage flow:
  $ zombiectl install lead-collector
  $ zombiectl credential add agentmail
    Agentmail API key: ************
    Stored in vault.
  $ zombiectl up
    lead-collector is live.
    Listening for emails at you@mail.usezombie.com
```

**Dimensions (test blueprints):**
- 5.1 PENDING
  - target: `zombiectl/src/commands/install.js`
  - input: `zombiectl install lead-collector`
  - expected: `Lead Zombie TOML config written to project dir, success message printed, bundled template used`
  - test_type: unit
- 5.2 PENDING
  - target: `zombiectl/src/commands/up.js`
  - input: `zombiectl up` with valid config + sample workspace
  - expected: `Config deployed to UseZombie cloud via API, Zombie starts, status URL printed`
  - test_type: integration (API)
- 5.3 PENDING
  - target: `zombiectl/src/commands/credential.js`
  - input: `zombiectl credential add agentmail` with valid API key
  - expected: `Credential stored in vault via API, confirmation printed`
  - test_type: integration (API)
- 5.4 PENDING
  - target: `zombiectl/src/commands/status.js`
  - input: `zombiectl status` with running Zombie
  - expected: `Zombie name, status (active/paused/error), uptime, events processed, budget used ($)`
  - test_type: integration (API)

---

## 6.0 Database Schema

**Status:** PENDING

Three new tables following `docs/SCHEMA_CONVENTIONS.md`: UUIDv7 IDs, BIGINT timestamps, schema-qualified names, ≤100 lines per file. Registered in `schema/embed.zig` and `src/cmd/common.zig`.

**Dimensions (test blueprints):**
- 6.1 PENDING
  - target: `schema/022_core_zombies.sql`
  - input: `INSERT valid Zombie row with UUIDv7 id, workspace_id FK, name, config JSONB, status='active'`
  - expected: `Row inserted, UUIDv7 check passes, FK constraint holds`
  - test_type: integration (DB)
- 6.2 PENDING
  - target: `schema/023_core_zombie_sessions.sql`
  - input: `INSERT session with zombie_id FK, context JSONB, checkpoint_at BIGINT`
  - expected: `Row inserted; UPSERT on zombie_id updates context and checkpoint_at`
  - test_type: integration (DB)
- 6.3 PENDING
  - target: `schema/024_core_activity_events.sql`
  - input: `INSERT activity event; attempt UPDATE on existing row`
  - expected: `INSERT succeeds; UPDATE raises exception (append-only trigger)`
  - test_type: integration (DB)

---

## 7.0 Interfaces

**Status:** PENDING

### 7.1 Public Functions

```zig
// src/zombie/config.zig
pub const ZombieConfig = struct {
    name: []const u8,
    trigger: TriggerConfig,
    tools: []const []const u8,
    credentials: []const []const u8,  // op:// vault references
    policy: ?PolicyConfig,
    budget: BudgetConfig,
};

pub fn parseZombieConfig(allocator: Allocator, toml_bytes: []const u8) !ZombieConfig
pub fn validateToolAttachments(config: ZombieConfig, tool_registry: *ToolRegistry) !void

// src/zombie/event_loop.zig
pub fn claimZombie(zombie_id: types.ZombieId, pool: *pg.Pool, redis: *Redis) !ZombieSession
pub fn runEventLoop(session: *ZombieSession, executor: *Executor, stream: *ActivityStream) !void
pub fn deliverEvent(session: *ZombieSession, event: WebhookEvent) !EventResult
pub fn checkpointState(session: *ZombieSession, pool: *pg.Pool) !void

// src/zombie/activity_stream.zig
pub fn logEvent(pool: *pg.Pool, event: ActivityEvent) void  // never errors to caller
pub fn queryByZombie(pool: *pg.Pool, zombie_id: types.ZombieId, cursor: ?[]const u8, limit: u32) !ActivityPage
pub fn queryByWorkspace(pool: *pg.Pool, workspace_id: types.WorkspaceId, cursor: ?[]const u8, limit: u32) !ActivityPage

// src/http/handlers/webhooks.zig
pub fn receiveWebhook(ctx: *RequestContext) !void
```

### 7.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `zombie_id` | UUID | UUIDv7, must exist in core.zombies | `019...` |
| `event_id` | Text | Non-empty, max 256 bytes, unique per source | `evt_abc123` |
| `trigger.type` | Enum | `webhook` \| `cron` \| `api` | `webhook` |
| `trigger.source` | Text | Known source: `agentmail` \| `github` \| `slack` | `agentmail` |
| `tools[]` | Text[] | Each must exist in tool registry | `["agentmail"]` |
| `budget.daily_dollars` | f64 | > 0, max 1000.0 | `5.00` |
| `budget.monthly_dollars` | f64 | > 0, max 10000.0 | `29.00` |

### 7.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `event_result.status` | Enum | After event delivery | `processed` \| `skipped_duplicate` \| `error` |
| `event_result.agent_response` | Text | On successful processing | `"Replied to lead with invite code"` |
| `activity_page.events` | ActivityEvent[] | On query | `[{type, detail, timestamp}]` |
| `activity_page.next_cursor` | ?Text | If more pages | `"019abc..."` |

### 7.4 Error Contracts

| Error condition | Code | Developer sees (human-readable) | HTTP |
|----------------|------|--------------------------------|------|
| Webhook for unknown source | `UZ-WH-001` | "No Zombie is listening for 'agentmail' webhooks. Create one with: zombiectl install lead-collector" | 404 |
| Malformed webhook payload | `UZ-WH-002` | "Webhook payload is not valid JSON. Check the request body." | 400 |
| Zombie paused | `UZ-WH-003` | "Zombie 'lead-collector' is paused. Resume with: zombiectl up" | 409 |
| Zombie budget exceeded | `UZ-ZMB-001` | "Zombie 'lead-collector' hit its daily budget ($5.00). Increase with: zombiectl config set budget.daily_dollars 10" | 402 |
| Agent timeout in sandbox | `UZ-ZMB-002` | Activity: "Agent timed out after 300s processing email from user@example.com" | — |
| Credential not found | `UZ-ZMB-003` | "Credential 'agentmail_api_key' not found. Add it with: zombiectl credential add agentmail" | — |
| Redis unavailable | `UZ-SYS-001` | "UseZombie is temporarily unavailable. Retrying..." | 503 |

---

## 8.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Duplicate webhook | External service retries | Idempotency dedup via event_id in Redis SET | Silent (correct) |
| Zombie crash | Agent OOM, executor failure, worker panic | Auto-restart from last Postgres checkpoint | Brief gap, then resumes |
| Agent timeout | LLM provider slow/down, infinite loop | Budget gate kills after max_wall_time | Activity: "agent timed out" |
| Webhook flood | External service mass-fires | Rate limit at API layer (existing RATE_LIMIT_CAPACITY) | HTTP 429 for excess |
| Activity write failure | Postgres connection lost | logEvent returns silently, Zombie continues | Audit gap (logged to stderr) |
| Credential not found | Vault reference points to missing item | Zombie fails to start, error logged | CLI: "credential not found: op://..." |
| Redis disconnect | Network partition, Redis restart | Worker backoff + reconnect loop | Webhooks queue at sender, resume on reconnect |

**Platform constraints:**
- Executor lease timeout (default 300s) bounds max agent execution per event
- bwrap network policy must include agentmail API domain in allowlist for Lead Zombie
- Redis consumer group requires XACK after processing — unacked events redeliver on crash

---

## 9.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Every new file < 500 lines | `wc -l src/zombie/*.zig \| awk '$1 > 500 {print "FAIL:" $2}'` |
| Each schema file ≤ 100 lines, single-concern | `wc -l schema/02[2-4]*.sql` |
| Schema files registered in embed.zig + common.zig | `grep -c '02[2-4]' schema/embed.zig src/cmd/common.zig` |
| Cross-compiles on x86_64-linux, aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| No heap allocations in webhook hot path (receive → enqueue) | Benchmark: `make bench` with webhook load |
| drain() before deinit() on all pg query results | `make check-pg-drain` |
| Schema-qualified table names in all new SQL | `grep -c 'core\.' schema/02[2-4]*.sql` |
| UUIDv7 CHECK constraint on every new table | `grep 'ck_.*uuidv7' schema/02[2-4]*.sql` |
| BIGINT NOT NULL for all timestamps | `grep -c 'TIMESTAMPTZ\|TIMESTAMP\|DEFAULT now' schema/02[2-4]*.sql` (must be 0) |
| Activity events table is append-only (trigger) | Dimension 5.3 test |
| At-least-once delivery: Redis XACK after processing only | Code review of event_loop.zig |
| Budget in dollars, not tokens (user-facing) | Config schema validation |

---

## 10.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| parse_valid_config | 1.1 | config.zig:parseZombieConfig | Valid TOML | ZombieConfig struct |
| parse_missing_name | 1.2 | config.zig:parseZombieConfig | TOML without name | error.MissingRequiredField |
| validate_unknown_tool | 1.3 | config.zig:validateToolAttachments | tools=["bad"] | error.UnknownTool |
| parse_invalid_trigger | 1.4 | config.zig:parseZombieConfig | trigger.type="bad" | error.InvalidTriggerType |
| receive_malformed_webhook | 2.4 | webhooks.zig:receiveWebhook | Invalid JSON | HTTP 400 |
| cli_install_bundled | 5.1 | install.js | zombiectl install lead-collector | Config written, success message |
| cli_up_deploys | 5.2 | up.js | zombiectl up (sample workspace) | Zombie starts on cloud |
| cli_credential_add | 5.3 | credential.js | zombiectl credential add agentmail | Stored in vault |
| cli_status_running | 5.4 | status.js | zombiectl status | Name, status, budget shown |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| webhook_enqueue | 2.1 | Redis | Valid agentmail webhook | Event on Redis stream |
| webhook_idempotent | 2.2 | Redis | Duplicate event_id | No re-enqueue |
| webhook_no_zombie | 2.3 | DB | Webhook for unregistered source | HTTP 404 |
| claim_zombie | 3.1 | DB + Redis + Executor | Unclaimed zombie_id | Session initialized |
| deliver_event | 3.2 | DB + Redis + Executor | Webhook event | Agent processes, checkpoint written |
| crash_recovery | 3.3 | DB + Executor | Zombie with checkpoint | Agent resumes from checkpoint |
| deliver_duplicate | 3.4 | DB + Redis | Same event_id twice | Idempotent handling |
| log_activity | 4.1 | DB | Activity event | Row in activity_events |
| log_failure_safe | 4.2 | DB | Simulated write failure | No crash, Zombie continues |
| query_by_zombie | 4.3 | DB | zombie_id + cursor | Paginated results |
| query_by_workspace | 4.4 | DB | workspace_id + cursor | Cross-Zombie results |
| zombies_crud | 6.1 | DB | INSERT valid row | UUIDv7 check passes |
| sessions_upsert | 6.2 | DB | INSERT then UPSERT | Context updated |
| activity_append_only | 6.3 | DB | INSERT then UPDATE | UPDATE raises exception |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "receives inbound email webhooks" | webhook_enqueue | integration (Redis) |
| "processes via NullClaw agent in sandbox" | deliver_event | integration (full stack) |
| "replies via agentmail API with credentials injected" | deliver_event (verify agent tool call) | integration (full stack) |
| "logs every action to activity stream" | log_activity | integration (DB) |
| "Zombie remembers leads across conversations" | crash_recovery | integration (DB + Executor) |
| "at-least-once delivery" | deliver_duplicate | integration (DB + Redis) |
| "2-command hero: install + up" | cli_install_bundled + cli_up_deploys | unit + integration (API) |
| "sample workspace works with zero credentials" | cli_up_deploys (sample workspace) | integration (API) |
| "credential add stores in vault" | cli_credential_add | integration (API) |

---

## 11.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Create schema files (022, 023, 024) + register in embed.zig + common.zig | `zig build` compiles |
| 2 | Implement ZombieConfig parser (src/zombie/config.zig) | Unit tests 1.1-1.4 pass |
| 3 | Implement ActivityStream writer + query (src/zombie/activity_stream.zig) | Integration tests 4.1-4.4 pass |
| 4 | Implement webhook handler (src/http/handlers/webhooks.zig) + register route | Integration tests 2.1-2.4 pass |
| 5 | Implement Zombie event loop (src/zombie/event_loop.zig) importing v1 budget/kill primitives | Integration tests 3.1-3.4 pass |
| 6 | Add agentmail domain to executor network allowlist | Verify via executor preflight |
| 7 | Bundle lead-collector TOML template in zombiectl npm package | `ls zombiectl/templates/lead-collector.toml` |
| 8 | Implement CLI commands: install, up, status, kill, logs, credential add/list | Unit tests 5.1-5.4 pass |
| 9 | Create sample workspace with pre-configured demo credentials | `zombiectl up` works with no credential add |
| 10 | End-to-end hero flow test | `zombiectl install lead-collector && zombiectl up` → send email → Zombie replies |
| 11 | Cross-compile check (Zig only) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| 12 | Full test suite | `make test && make test-integration && make lint` |

---

## 12.0 Acceptance Criteria

**Status:** PENDING

- [ ] **Hero flow works in < 2 min:** `zombiectl install lead-collector && zombiectl up` → Zombie is live — verify: timed manual test
- [ ] **Sample workspace demo:** `zombiectl up` with no credential add works (demo mode) — verify: manual test
- [ ] **Real credentials flow:** `zombiectl credential add agentmail` + `zombiectl up` → Zombie replies to real emails — verify: send email, check reply
- [ ] **CLI commands:** install, up, status, kill, logs, credential add, credential list all work — verify: `bun test zombiectl/test/`
- [ ] `make test` passes with >= 9 new unit tests (5 Zig + 4 CLI) — verify: `make test`
- [ ] `make test-integration` passes with >= 10 new integration tests — verify: `make test-integration`
- [ ] `make lint` passes — verify: `make lint`
- [ ] Cross-compile succeeds — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `make check-pg-drain` passes — verify: `make check-pg-drain`
- [ ] All new files < 500 lines — verify: `git diff --name-only origin/main | xargs wc -l | awk '$1 > 500'`
- [ ] Activity stream shows webhook_received + agent_response events — verify: `zombiectl activity list`
- [ ] Zombie crash + restart recovers conversation context — verify: kill worker, restart, query session
- [ ] Every error message includes: problem + cause + fix command — verify: code review of error contracts

---

## 13.0 Verification Evidence

**Status:** PENDING

Filled in during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint | `make lint` | | |
| 500L gate | `git diff --name-only origin/main \| xargs wc -l` | | |
| pg-drain | `make check-pg-drain` | | |
| End-to-end email | Manual test | | |
| Activity stream | `zombiectl activity list` | | |
| Crash recovery | Kill + restart worker | | |
| Hero flow (< 2 min) | Timed: install + up | | |
| CLI install | `zombiectl install lead-collector` | | |
| CLI status | `zombiectl status` | | |
| CLI credential | `zombiectl credential add agentmail` | | |
| Error messages | Code review: problem + cause + fix | | |

---

## 14.0 Out of Scope

- Approval gate (M27_002 — skill attachment, sandbox-enforced)
- Slack integration (M27_002)
- Git/GitHub tool attachments (M27_003)
- Multi-step pipeline / always-on listener (M27_003)
- Web dashboard (M27_004)
- Full CLI reskin (M27_004 — only minimal zombie commands ship in Phase 1)
- External agent execute endpoint (future)
- Anomaly detection (M27_002)
- Credential lifecycle (rotation, revocation — M27_002 spec dimension)
- Multi-tenant abuse controls (M27_003 spec dimension)
- Soak/chaos tests (M27_003 verification)
- Interactive playground on usezombie.com (ships alongside or after M27_002)
- Remote registry for Zombie templates / ClawHub (Phase 3)
- Docs site at docs.usezombie.com (M27_004)

## 15.0 DX Decisions (from /plan-devex-review)

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Minimal CLI commands ship in Phase 1 | Hero flow (`install` + `up`) can't wait for Phase 4 |
| 2 | Flat top-level for common ops | `zombiectl install`, not `zombiectl zombie install`. Namespaced for less common: `credential add` |
| 3 | Zombie templates bundled in npm package | `install lead-collector` works offline, no registry dependency |
| 4 | Remote registry support planned | ClawHub in Phase 3, bundled + remote both supported |
| 5 | Sample workspace for hero demo | Zero credential setup. Demo mode works out of the box. |
| 6 | Remote-first execution | `zombiectl up` deploys to UseZombie cloud. Mac developers never need bwrap. |
| 7 | Separate credential add step | `zombiectl credential add agentmail` is explicit. Hero uses sample workspace. |
| 8 | Budget in dollars, not tokens | Developer sees "$4.23 spent today", not "47,000 tokens" |
| 9 | Every error includes problem + cause + fix | "Credential 'agentmail_api_key' not found. Add it with: zombiectl credential add agentmail" |
| 10 | TTHW target: < 2 minutes | Champion tier. Competitive with Modal (2 min), Replicate (1 min) |
