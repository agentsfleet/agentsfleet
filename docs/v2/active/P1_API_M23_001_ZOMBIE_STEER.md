# P1_API_M23_001: Zombie Steer — Live Chat Steering + Execution Tracking

**Prototype:** v2
**Milestone:** M23
**Workstream:** 001
**Date:** Apr 14, 2026
**Status:** IN_PROGRESS
**Branch:** feat/m23-001-zombie-steer
**Priority:** P1 — Operators cannot redirect a running zombie without killing it. Live chat steering closes the feedback loop for long-running (ops, research) zombies.

---

## Overview

**Goal (testable):** An operator sends `POST /v1/zombies/{id}:steer` with a message. The message is written to Redis key `zombie:{id}:steer` (TTL 300s). The worker polls this key at the top of each event loop iteration; if found, it injects the message as a synthetic `steer` event into the zombie's event stream, where it is delivered by the normal path.

Additionally, `core.zombie_sessions` tracks `execution_id` and `execution_started_at` so operators and the API can tell whether a zombie is currently executing and when that execution started.

**Scope (confirmed Apr 14):**
- ✅ Live steering: operator message injected as synthetic event via Redis → worker → XADD
- ✅ Execution tracking: `execution_id` + `execution_started_at` on `zombie_sessions` — set at `createExecution`, cleared at `destroyExecution`
- ✅ Steer endpoint returns `execution_id` (null if zombie is idle) so caller can see whether message lands mid-execution or queued
- ❌ No memory persistence across runs (M14_001 parked)
- ❌ No LLM scope inference
- ❌ No SSE event emission
- ❌ True mid-execution injection while LLM is streaming (requires `injectUserMessage` — deferred to M24)

---

## Architecture

### Steer signal flow

```
POST /v1/zombies/{id}:steer
  → handler verifies ownership
  → SETEX zombie:{id}:steer "{message}" 300
  → returns {ack: true, run_steered: bool, execution_id: string|null}

Worker event loop (per iteration, before pollNextEvent):
  → GETDEL zombie:{id}:steer
  → if found: XADD zombie:{id}:events {event_id, type="steer", source="operator", data=msg}
  → next loop iteration: picks up steer event from stream, delivers normally
```

### Execution tracking

```
executeInSandbox():
  createExecution() → setExecutionActive(session, execution_id)
    → DB: UPDATE zombie_sessions SET execution_id=$1, execution_started_at=$2
    → session.execution_id = owned copy
  defer clearExecutionActive(session)
    → DB: UPDATE zombie_sessions SET execution_id=NULL, execution_started_at=NULL
    → session.execution_id = null

claimZombie() → clearExecutionActive(session)  [crash recovery: clear stale state]
```

---

## Files Changed

| File | Action | Why |
|------|--------|-----|
| `schema/023_core_zombie_sessions.sql` | MODIFY | Add `execution_id TEXT NULL` + `execution_started_at BIGINT NULL` |
| `src/queue/constants.zig` | MODIFY | Add `zombie_steer_key_suffix` + `zombie_steer_ttl_seconds` |
| `src/queue/redis_client.zig` | MODIFY | Add `getDel` method |
| `src/zombie/event_loop_types.zig` | MODIFY | Add `execution_id` + `execution_started_at` to `ZombieSession` |
| `src/zombie/event_loop_helpers.zig` | MODIFY | Add `setExecutionActive`, `clearExecutionActive`, `pollSteerAndInject`; receive `executeInSandbox` from event_loop.zig |
| `src/zombie/event_loop.zig` | MODIFY | Remove `executeInSandbox` (moved to helpers); add steer poll in while loop; clear stale execution on claim |
| `src/http/handlers/zombie_steer_http.zig` | MODIFY | Rewrite: use `zombie:{id}:steer` key; query real `execution_id` column |
| `src/http/route_matchers.zig` | (done) | `matchZombieAction` helper |
| `src/http/router.zig` | (done) | `zombie_steer` route variant |
| `src/http/route_table.zig` | (done) | bearer middleware registration |
| `src/http/route_table_invoke.zig` | (done) | invoke shim |
| `public/openapi.json` | MODIFY | Update endpoint description |
| `docs/v1/` | DELETE | v1-era specs conflict with v2 architecture |

---

## Applicable Rules

- **RULE FLS** — drain every pg query before deinit.
- **RULE FLL** — 350-line gate. `event_loop.zig` was at 352 lines; `executeInSandbox` moved to helpers.
- **RULE NSQ** — schema-qualified SQL (`core.zombie_sessions`, `core.zombies`).

---

## §1 — `POST /v1/zombies/{id}:steer` Endpoint

**Status:** IN_PROGRESS

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `zombie_steer_http.zig` | POST valid bearer, zombie idle | 200 `{message_queued:true, execution_active:false, execution_id:null}` | unit |
| 1.2 | PENDING | `zombie_steer_http.zig` | POST valid bearer, zombie has active execution_id | 200 `{message_queued:true, execution_active:true, execution_id:"..."}` | unit |
| 1.3 | PENDING | `zombie_steer_http.zig` | POST missing auth | 401 | unit |
| 1.4 | PENDING | `zombie_steer_http.zig` | POST cross-workspace zombie | 404 | unit |
| 1.5 | PENDING | `zombie_steer_http.zig` | POST empty message | 400 | unit |
| 1.6 | PENDING | `zombie_steer_http.zig` | POST message > 8192 bytes | 400 | unit |

---

## §2 — Execution Tracking

**Status:** IN_PROGRESS

| Dim | Status | Target | Expected | Test type |
|-----|--------|--------|----------|-----------|
| 2.1 | PENDING | `setExecutionActive` | `zombie_sessions.execution_id` set when execution starts | unit |
| 2.2 | PENDING | `clearExecutionActive` | `zombie_sessions.execution_id = NULL` when execution ends | unit |
| 2.3 | PENDING | `claimZombie` | stale execution_id cleared on worker restart | unit |

---

## §3 — Worker Steer Poll

**Status:** IN_PROGRESS

| Dim | Status | Target | Expected | Test type |
|-----|--------|--------|----------|-----------|
| 3.1 | PENDING | `pollSteerAndInject` | GETDEL `zombie:{id}:steer` → XADD with type="steer" | unit |
| 3.2 | PENDING | `runEventLoop` | poll called before each `pollNextEvent` | unit |

---

## Interfaces

### Public Endpoint

```
POST /v1/zombies/{zombie_id}:steer
Headers: Authorization: Bearer {workspace_token}
Body: {
  "message": string (1..8192)
}
Response 200: {
  "message_queued": boolean,   // true if Redis write succeeded
  "execution_active": boolean, // true if zombie has an active execution_id
  "execution_id": string | null
}
```

Note: `:steer` uses the Google Custom Methods colon-action pattern intentionally — it is an RPC-style action on a resource, not a CRUD operation. This deviates from §7 "avoid verbs" by design.

### Error Contracts

| Condition | Behavior |
|-----------|----------|
| Missing auth | 401 `UZ-AUTH-002` |
| Wrong workspace | 404 `UZ-ZOMBIE-NOT-FOUND` |
| Empty message | 400 `UZ-REQ-001` |
| Message > 8192 | 400 `UZ-REQ-001` |
| Redis write fails | 200 with `run_steered: false`, logs warn |

### Schema Columns (core.zombie_sessions)

| Column | Type | Nullable | Set by | Cleared by |
|--------|------|----------|--------|------------|
| `execution_id` | TEXT | YES | worker `setExecutionActive` | `clearExecutionActive`, `claimZombie` |
| `execution_started_at` | BIGINT | YES | worker `setExecutionActive` | `clearExecutionActive`, `claimZombie` |

---

## Acceptance Criteria

- [ ] `POST /v1/zombies/{id}:steer` returns 200 with `message_queued: true, execution_active: false` when zombie is idle
- [ ] Returns `message_queued: true, execution_active: true` + execution_id when zombie is executing
- [ ] Returns `message_queued: false` when Redis write fails
- [ ] Redis key `zombie:{id}:steer` written with 300s TTL
- [ ] Worker injects steer message as type="steer" event into zombie event stream
- [ ] execution_id set in zombie_sessions at createExecution, cleared at destroyExecution
- [ ] Stale execution_id cleared on worker restart
- [ ] Returns 404 for cross-workspace zombie
- [ ] Returns 401 for missing auth
- [ ] Returns 400 for empty or oversized message
- [ ] `event_loop.zig` ≤ 350 lines
- [ ] `event_loop_helpers.zig` ≤ 350 lines
- [ ] `make test` passes

---

## Out of Scope

- Memory persistence across runs (M14_001 parked)
- True mid-execution injection while LLM is streaming (M24 — `injectUserMessage`)
- UI chat panel (M24)
