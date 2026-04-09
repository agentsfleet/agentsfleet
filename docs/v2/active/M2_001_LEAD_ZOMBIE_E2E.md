# M2_001: Lead Zombie E2E — hero flow works end-to-end, approval gate foundation

**Prototype:** v0.6.0
**Milestone:** M2
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — First live Zombie; proves the pitch works
**Batch:** B1 — no dependencies
**Branch:** feat/m2-lead-zombie-e2e
**Depends on:** M1_001 (schema, config, event loop, CLI, webhook ingestion)

---

## Overview

**Goal (testable):** `zombiectl install lead-collector && zombiectl up` produces a live Zombie that receives a real agentmail webhook, processes it in the NullClaw sandbox, replies via agentmail API with vault-injected credentials, and logs every step to the activity stream visible via `zombiectl logs`. A pre-configured sample workspace lets the hero demo run with zero credential setup. Crash-and-resume recovers conversation context from Postgres checkpoint.

**Problem:** M1_001 built all the modules (event loop, CLI, webhook handler, activity stream) but they haven't been wired end-to-end. The CLI commands talk to mocked APIs in tests. The event loop has unit tests but no live executor integration. The hero flow (`install + up → send email → Zombie replies`) doesn't work yet. Without a working demo, we can't onboard the first user or record the pitch video.

**Solution summary:** Wire the event loop into the worker process (attach to worker fleet's run loop), add the agentmail domain to the executor's network allowlist, create a sample workspace with demo credentials, add `zombiectl activity list` route, and run the full E2E hero flow. Also lay the foundation for the approval gate (Slack webhook integration) which is the next acquisition hook after "agent is live."

---

## 1.0 Worker Integration

**Status:** PENDING

Wire `event_loop.zig` into the worker fleet's main run loop. The worker already claims specs via Redis; extend it to also claim Zombies. Worker process starts → checks for Zombie assignments → calls `claimZombie` → enters `runEventLoop`. One worker instance handles one Zombie (1:1 for v0.6.0; multiplexing is M3).

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/cmd/worker.zig:runWorker`
  - input: `Worker starts with a Zombie assigned in core.zombies`
  - expected: `Worker claims Zombie, enters event loop, processes webhook events`
  - test_type: integration (DB + Redis + Executor)
- 1.2 PENDING
  - target: `src/cmd/worker.zig:runWorker`
  - input: `Worker starts with no Zombie assignment`
  - expected: `Worker polls for assignment, sleeps, does not crash`
  - test_type: integration (DB)
- 1.3 PENDING
  - target: `src/zombie/event_loop.zig:runEventLoop`
  - input: `Zombie receives SIGTERM while processing event`
  - expected: `Event loop checkpoints state, drains cleanly, exits 0`
  - test_type: integration (DB + Redis)
- 1.4 PENDING
  - target: `src/cmd/worker.zig:runWorker`
  - input: `Worker crashes mid-event, restarts`
  - expected: `Worker reclaims Zombie from checkpoint, resumes processing from last XACK'd event`
  - test_type: integration (DB + Redis + Executor)

---

## 2.0 Executor Network Allowlist + Credential Injection

**Status:** PENDING

Add agentmail API domain (`api.agentmail.dev`) to the executor's network allowlist for Lead Zombie. Verify credential injection from vault into the sandbox environment (the executor already supports this via M16_003; this wires it for the Zombie use case). The Zombie's `credentials` array in config maps to `op://` vault paths; the executor resolves them at sandbox start, not at config parse time.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/executor/network_policy.zig` (or equivalent)
  - input: `Lead Zombie config with skills=["agentmail"], network.allow=["api.agentmail.dev"]`
  - expected: `Executor allowlists api.agentmail.dev in bwrap network namespace`
  - test_type: integration (Executor)
- 2.2 PENDING
  - target: `src/executor/credential_inject.zig` (or equivalent)
  - input: `Zombie config with credentials=["op://ZMB_LOCAL_DEV/agentmail/api_key"]`
  - expected: `Credential resolved from vault, injected as env var inside sandbox, never visible in agent code`
  - test_type: integration (Executor + Vault)
- 2.3 PENDING
  - target: `src/executor/network_policy.zig`
  - input: `Agent inside sandbox attempts to reach api.stripe.com (not in allowlist)`
  - expected: `Connection refused, error logged to activity stream`
  - test_type: integration (Executor)
- 2.4 PENDING
  - target: `src/executor/credential_inject.zig`
  - input: `Zombie config references credential not in vault`
  - expected: `Zombie fails to start, error UZ-ZMB-003 with actionable hint`
  - test_type: integration (Executor + Vault)

---

## 3.0 Sample Workspace + Hero Flow

**Status:** PENDING

Create `samples/lead-collector/` with a pre-configured workspace that enables the hero demo with zero credential setup. Demo mode uses a sandbox agentmail account. `zombiectl up` detects demo credentials and prints "demo mode" banner. The full hero flow runs in under 2 minutes.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `samples/lead-collector/`
  - input: `Fresh clone, no credentials configured`
  - expected: `zombiectl install lead-collector && zombiectl up` works with demo account, prints demo mode banner
  - test_type: E2E (manual + scripted)
- 3.2 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandUp`
  - input: `zombiectl up` with real credentials (after `credential add`)
  - expected: `Zombie starts in production mode, no demo banner`
  - test_type: E2E (manual)
- 3.3 PENDING
  - target: E2E flow
  - input: `Send email to demo@mail.usezombie.com`
  - expected: `Zombie receives webhook, processes email, replies via agentmail, activity stream shows webhook_received + agent_response`
  - test_type: E2E (manual, timed < 2 min)
- 3.4 PENDING
  - target: `src/zombie/event_loop.zig:checkpointState` + `claimZombie`
  - input: `Kill worker mid-processing, restart`
  - expected: `Zombie recovers from Postgres checkpoint, resumes with conversation context intact`
  - test_type: E2E (manual)

---

## 4.0 Activity Stream CLI Route

**Status:** DONE

Add `zombiectl activity list` (aliased as `zombiectl logs`) server-side route. Currently `zombiectl logs` calls the API but there's no server handler. Implement `GET /v1/zombies/activity` with cursor-based pagination, returning the last N activity events. Wire the existing `activity_stream.zig:queryByZombie` to the HTTP handler.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleListActivity`
  - input: `GET /v1/zombies/activity?zombie_id={id}&limit=20`
  - expected: `JSON array of activity events, cursor for next page`
  - test_type: integration (DB)
- 4.2 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleListActivity`
  - input: `GET /v1/zombies/activity?zombie_id={id}&cursor={cursor}&limit=20`
  - expected: `Next page of events, empty array when no more`
  - test_type: integration (DB)
- 4.3 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleListActivity`
  - input: `GET /v1/zombies/activity?zombie_id={unknown_id}`
  - expected: `Empty array (not 404 — Zombie may exist with no events)`
  - test_type: integration (DB)
- 4.4 DONE
  - target: `src/http/router.zig`
  - input: `GET /v1/zombies/activity` registered in router`
  - expected: `Route matches, auth required, workspace scoped`
  - test_type: unit

---

## 5.0 Zombie CRUD API

**Status:** DONE

Server-side handlers for `zombiectl up`, `zombiectl status`, `zombiectl kill`, and `zombiectl credential add/list`. The CLI commands already call these endpoints; implement the handlers. `POST /v1/zombies/` creates a Zombie from config JSON + source markdown. `GET /v1/zombies/` lists Zombies for workspace. `DELETE /v1/zombies/{id}` kills a Zombie. `POST /v1/zombies/credentials` stores a credential in vault. `GET /v1/zombies/credentials` lists credential names (not values).

**Dimensions (test blueprints):**
- 5.1 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleCreateZombie`
  - input: `POST /v1/zombies/ with valid config JSON, name, workspace_id`
  - expected: `Zombie row created in core.zombies, status=active, zombie_id returned`
  - test_type: integration (DB)
- 5.2 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleListZombies`
  - input: `GET /v1/zombies/?workspace_id={id}`
  - expected: `JSON array of Zombies with name, status, events_processed, budget_used_dollars`
  - test_type: integration (DB)
- 5.3 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleDeleteZombie`
  - input: `DELETE /v1/zombies/{id}`
  - expected: `Zombie status set to killed, worker notified to drain`
  - test_type: integration (DB + Redis)
- 5.4 PENDING
  - target: `src/http/handlers/zombie_api.zig:handleStoreCredential`
  - input: `POST /v1/zombies/credentials with name=agentmail, value=sk-xxx, workspace_id`
  - expected: `Credential stored in vault, name returned (not value)`
  - test_type: integration (DB + Vault)

---

## 6.0 Interfaces

**Status:** PENDING

### 6.1 Public Functions

```zig
// src/http/handlers/zombie_api.zig
pub fn handleCreateZombie(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void
pub fn handleListZombies(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void
pub fn handleDeleteZombie(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void
pub fn handleListActivity(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void
pub fn handleStoreCredential(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void
pub fn handleListCredentials(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void
```

### 6.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `name` | Text | Non-empty, max 64 chars, slug-safe | `lead-collector` |
| `config_json` | JSONB | Valid ZombieConfig JSON | `{"trigger":...}` |
| `source_markdown` | Text | Non-empty, max 64KB | `---\ntrigger:...` |
| `workspace_id` | UUID | UUIDv7, must exist | `019...` |
| `credential.name` | Text | Non-empty, max 64 chars | `agentmail` |
| `credential.value` | Text | Non-empty, max 4KB | `sk-xxx` |
| `limit` | u32 | 1–100, default 20 | `20` |
| `cursor` | ?Text | Decimal string of created_at | `1712678400000` |

### 6.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `zombie_id` | UUID | On create | `019abc...` |
| `status` | Enum | On list/create | `active` \| `paused` \| `killed` |
| `events_processed` | u64 | On list | `42` |
| `budget_used_dollars` | f64 | On list | `1.23` |
| `credentials[].name` | Text | On list credentials | `agentmail` |
| `credentials[].created_at` | Text | On list credentials | `2026-04-09` |

### 6.4 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| Zombie name already exists in workspace | `UZ-ZMB-006` | "Zombie 'lead-collector' already exists. Use `zombiectl kill` first." | 409 |
| Workspace not found | `UZ-WS-001` | "Workspace not found. Run `zombiectl workspace list`." | 404 |
| Credential value too long | `UZ-ZMB-007` | "Credential value exceeds 4KB limit." | 400 |
| Invalid config JSON | `UZ-ZMB-008` | "Config JSON is not valid. Check trigger, skills, and budget fields." | 400 |

---

## 7.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Worker crash mid-event | OOM, executor panic | Zombie reclaimed from checkpoint on restart | Brief gap, then resumes |
| Agentmail API down | External dependency | Agent timeout, event retried on next XREADGROUP | Activity: "agent timed out" |
| Vault credential missing | User didn't run credential add | Zombie fails to start, UZ-ZMB-003 | CLI: "credential not found" |
| Demo account rate limited | Too many demo emails | Agent returns rate-limit error | Activity: "agentmail rate limit" |
| Zombie killed while processing | User runs `zombiectl kill` | Worker drains current event, checkpoints, exits | Status transitions to "killed" |
| Network allowlist violation | Agent tries blocked domain | Connection refused at bwrap level | Activity: "network policy denied" |

**Platform constraints:**
- bwrap network namespace requires root on some Linux distros (CI uses Docker with --privileged)
- agentmail sandbox account has 100 emails/day rate limit for demo mode
- Worker lease timeout (300s) bounds max agent execution per event

---

## 8.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Every new file < 500 lines | `git diff --name-only origin/main \| xargs wc -l \| awk '$1 > 500'` |
| Cross-compiles on x86_64-linux, aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| drain() before deinit() on all pg query results | `make check-pg-drain` |
| Hero flow completes in < 2 minutes | Timed manual test |
| No heap allocations in webhook receive → enqueue path | Benchmark test or allocator audit |
| Sample workspace works without real credentials | `zombiectl install lead-collector && zombiectl up` with no `credential add` |
| Credential values never appear in logs, activity stream, or API responses | Code review + grep for credential value in response bodies |

---

## 9.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| create_zombie_valid | 5.1 | zombie_api.zig:handleCreateZombie | Valid POST body | 201 + zombie_id |
| list_zombies | 5.2 | zombie_api.zig:handleListZombies | GET with workspace_id | JSON array |
| delete_zombie | 5.3 | zombie_api.zig:handleDeleteZombie | DELETE with zombie_id | 200, status=killed |
| store_credential | 5.4 | zombie_api.zig:handleStoreCredential | POST name+value | 201, name returned |
| list_activity_empty | 4.3 | zombie_api.zig:handleListActivity | Unknown zombie_id | Empty array |
| route_registered | 4.4 | router.zig | GET /v1/zombies/activity | Route matches |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| worker_claims_zombie | 1.1 | DB + Redis + Executor | Worker starts | Zombie claimed, event loop running |
| worker_no_assignment | 1.2 | DB | Worker starts, no Zombie | Polls, no crash |
| worker_graceful_shutdown | 1.3 | DB + Redis | SIGTERM during event | Checkpoint, clean exit |
| worker_crash_recovery | 1.4 | DB + Redis + Executor | Kill + restart | Resumes from checkpoint |
| network_allowlist | 2.1 | Executor | Zombie with agentmail skill | api.agentmail.dev reachable |
| credential_inject | 2.2 | Executor + Vault | Zombie with op:// credential | Env var inside sandbox |
| network_deny | 2.3 | Executor | Agent calls blocked domain | Connection refused |
| credential_missing | 2.4 | Executor + Vault | Missing vault path | UZ-ZMB-003, no start |
| activity_pagination | 4.1-4.2 | DB | Multiple events + cursor | Paginated results |

### E2E Tests (Manual)

| Test name | Dimension | Steps | Expected |
|-----------|-----------|-------|----------|
| hero_flow_demo | 3.1 | install + up (no creds) | Demo mode, Zombie live |
| hero_flow_real | 3.2 | install + credential add + up | Production mode |
| email_round_trip | 3.3 | Send email → check reply | Reply received < 2 min |
| crash_resume | 3.4 | Kill worker → restart → query session | Context preserved |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "receives a real agentmail webhook" | email_round_trip | E2E |
| "processes it in the NullClaw sandbox" | worker_claims_zombie | integration |
| "replies via agentmail API with vault-injected credentials" | credential_inject + email_round_trip | integration + E2E |
| "logs every step to activity stream" | activity_pagination | integration |
| "sample workspace lets hero demo run with zero credential setup" | hero_flow_demo | E2E |
| "crash-and-resume recovers conversation context" | worker_crash_recovery + crash_resume | integration + E2E |
| "hero flow runs in under 2 minutes" | email_round_trip (timed) | E2E |

---

## 10.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Implement Zombie CRUD API handlers (create, list, delete) + register routes | Integration tests 5.1–5.3 pass |
| 2 | Implement credential store/list API handlers | Integration test 5.4 passes |
| 3 | Implement activity list API handler with cursor pagination | Integration tests 4.1–4.3 pass |
| 4 | Wire event loop into worker process (claim Zombie, enter loop) | Integration test 1.1 passes |
| 5 | Add agentmail domain to executor network allowlist | Integration test 2.1 passes |
| 6 | Wire credential injection for Zombie configs | Integration tests 2.2, 2.4 pass |
| 7 | Create samples/lead-collector/ with demo workspace | `zombiectl install lead-collector && zombiectl up` works |
| 8 | Add graceful shutdown to Zombie event loop | Integration test 1.3 passes |
| 9 | Test crash recovery end-to-end | Integration test 1.4 + E2E crash_resume pass |
| 10 | Full E2E hero flow | Send email → Zombie replies, timed < 2 min |
| 11 | Cross-compile check | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| 12 | Full test suite | `make test && make test-integration && make lint` |

---

## 11.0 Acceptance Criteria

**Status:** PENDING

- [ ] Hero flow works in < 2 min: `zombiectl install lead-collector && zombiectl up` → Zombie is live — verify: timed manual test
- [ ] Sample workspace demo: `zombiectl up` with no credential add works (demo mode) — verify: manual test
- [ ] Real credentials flow: `zombiectl credential add agentmail` + `zombiectl up` → Zombie replies to real emails — verify: send email, check reply
- [ ] CRUD API: create, list, delete Zombie via API — verify: `make test-integration`
- [ ] Activity stream: `zombiectl logs` shows webhook_received + agent_response events — verify: `zombiectl logs` after E2E test
- [ ] Crash recovery: kill worker → restart → Zombie resumes from checkpoint — verify: manual test
- [ ] Network deny: agent inside sandbox cannot reach non-allowlisted domains — verify: integration test
- [ ] Credential never in response: API response for credential list returns names only — verify: code review + test
- [ ] `make test` passes — verify: `make test`
- [ ] `make lint` passes — verify: `make lint`
- [ ] Cross-compile succeeds — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] All new files < 500 lines — verify: `git diff --name-only origin/main | xargs wc -l | awk '$1 > 500'`

---

## 12.0 Verification Evidence

**Status:** PENDING

Filled in during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| 500L gate | `git diff --name-only origin/main \| xargs wc -l` | | |
| pg-drain | `make check-pg-drain` | | |
| Hero flow (demo) | `zombiectl install + up` (no creds) | | |
| Hero flow (real) | Send email → receive reply | | |
| Crash recovery | Kill worker → restart → check session | | |
| Network deny | Agent calls blocked domain | | |
| Credential in response | grep credential value in API output | | |

---

## 13.0 Out of Scope

- Approval gate implementation (M2_002 — Slack webhook integration)
- Multi-Zombie per worker (M3 — multiplexing)
- Slack Bug Fixer Zombie (M3 — separate Zombie type)
- Git/GitHub tool attachments (M3)
- ClawHub / remote Zombie registry (Phase 3)
- Web dashboard (M4)
- Full CLI reskin (M4)
- External agent execute endpoint (future)
- Anomaly detection (M3)
- Credential lifecycle (rotation, revocation — M3)
