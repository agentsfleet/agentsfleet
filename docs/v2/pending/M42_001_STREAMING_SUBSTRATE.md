# M42_001: Streaming Substrate — Unified Event Ingest, History, Steer CLI, Live Watch SSE

**Prototype:** v2.0.0
**Milestone:** M42
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — launch-blocking. The "every event has provenance" + "operator can steer" + "operator can watch live" guarantees converge here. Without this, M37 platform-ops sample's chat experience is broken (no `zombiectl steer`, no event read-back, no live watch).
**Categories:** API, CLI, UI
**Batch:** B1 — parallel with M40, M41, M44, M45.
**Branch:** feat/m42-streaming-substrate (to be created)
**Depends on:** M40_001 (worker control stream — events flow into per-zombie threads claimed via M40's watcher). Independent of M41/M43.

**Canonical architecture:** `docs/ARCHITECHTURE.md` §8.3 (trigger modes), §9 (steer flow diagram), §10 (event stream + history capability), §12 step 7-9 (event ingest, history INSERT/UPDATE).

---

## Implementing agent — read these first

Before touching any file, read in this order:

1. `docs/ARCHITECHTURE.md` §9 + §12 — the canonical end-to-end sequence. Don't reinvent.
2. `src/http/handlers/zombies/steer_integration_test.zig` — the existing server-side `/steer` endpoint pattern. Mirror this for the new `/events` and `/activities/stream` endpoints.
3. `samples/platform-ops/SKILL.md` — what an actor=steer event's `data` field looks like in practice.
4. `zombiectl/src/commands/zombie.js` — existing CLI command pattern (look at how `install` is structured); `steer` and `events` subcommands match this shape.
5. `zombiectl/src/program/routes.js` — add `steer` + `events` routes here.

---

## Overview

**Goal (testable):** All four event sources (operator steer, GH webhook, NullClaw cron, M41 continuation) land on `zombie:{id}:events` with consistent envelope and `actor` provenance. `core.zombie_events` persists every event start (status='received') and end (status='processed' or 'agent_error') with `actor`, `request_json`, `response_text`, `tokens`, `wall_ms`. Operator runs `zombiectl steer {id} "morning health check"`, sees the zombie's response stream back inline, can Ctrl-C without killing the zombie. Operator runs `zombiectl events {id}` and sees a paginated history with actor filters. Operator opens dashboard `/zombies/{id}/live`, sees tool calls and responses streaming in real-time via SSE with <200ms latency.

**Problem:** Today there is no `core.zombie_events` table. `core.zombie_sessions` holds only the rolling context summary; per-event request/response is lost the moment the next event starts. Debugging means grepping logs. The CLI has no `steer` or `events` subcommands. The dashboard has no live activity stream. The webhook ingest path (M43) has nowhere to write — without M42's stream + history schema, M43 can't land.

**Solution summary:** One new table, one normalized event envelope, one write path through `processEvent`, three new HTTP endpoints, two new CLI subcommands, one SSE stream, one dashboard panel.

- **Schema**: `core.zombie_events` with `UNIQUE(zombie_id, event_id)` for idempotency; columns include `actor`, `event_type`, `request_json`, `response_text`, `tokens`, `wall_ms`, `failure_label`, `status`, `created_at`, `completed_at`.
- **Write path**: `processEvent` in worker INSERTs at top (status='received'), UPDATEs at bottom (status='processed' or 'agent_error'). Idempotent on event_id replay (M40's XAUTOCLAIM path).
- **Read endpoints**: `GET /v1/.../zombies/{id}/events?cursor=&actor=&limit=` (paginated history); `GET /v1/.../zombies/{id}/activities/stream` (SSE, tails `core.zombie_activities` which is a finer-grained per-tool-call log used by M36-style live watch).
- **CLI**: `zombiectl steer {id} [<msg>]` (interactive if no msg, batch if msg given); `zombiectl events {id} [--actor=<filter>]` (paginated history print).
- **UI**: dashboard `/zombies/{id}/live` panel consuming the SSE.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/0NN_zombie_events.sql` | NEW | Table definition; new schema slot per `docs/SCHEMA_CONVENTIONS.md` |
| `schema/embed.zig` | EXTEND | Register the new schema file |
| `src/cmd/common.zig` | EXTEND | Add to canonical migration array |
| `src/zombie/event_loop_helpers.zig` | EXTEND | `processEvent`: INSERT at top, UPDATE at bottom |
| `src/zombie/event_envelope.zig` | NEW | Normalized event envelope: encode/decode for stream + DB |
| `src/http/handlers/zombies/events.zig` | NEW | `GET /events` paginated history endpoint |
| `src/http/handlers/zombies/activities_stream.zig` | NEW | SSE endpoint tailing `core.zombie_activities` |
| `src/http/handlers/zombies/steer.zig` | EXTEND | (already exists per integration test) — verify error shapes match new envelope |
| `zombiectl/src/commands/zombie_steer.js` | NEW | `zombiectl steer` subcommand: interactive REPL + batch mode |
| `zombiectl/src/commands/zombie_events.js` | NEW | `zombiectl events` subcommand: paginated history print |
| `zombiectl/src/program/routes.js` | EXTEND | Register `steer` + `events` routes |
| `zombiectl/src/program/command-registry.js` | EXTEND | Wire handlers |
| `ui/packages/app/src/routes/zombies/[id]/live.tsx` | NEW | Dashboard live activity panel consuming SSE |
| `ui/packages/app/src/routes/zombies/[id]/events.tsx` | NEW | Dashboard events history table |
| `tests/integration/zombie_events_test.zig` | NEW | E2E: event lifecycle, idempotency on replay, actor filtering |
| `tests/integration/sse_live_watch_test.zig` | NEW | E2E: SSE delivers tool_call_started + tool_call_completed within 200ms |
| `samples/fixtures/m42-event-fixtures/` | NEW | Stable JSON payloads used by tests + dashboard storybook |

---

## Sections (implementation slices)

### §1 — Schema migration: `core.zombie_events`

```sql
CREATE TABLE core.zombie_events (
  zombie_id     uuid NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
  event_id      text NOT NULL,                          -- Redis stream entry id
  workspace_id  uuid NOT NULL,
  actor         text NOT NULL,                          -- 'steer:<user>' | 'webhook:<source>' | 'cron:<schedule>' | 'continuation:<original_actor>'
  event_type    text NOT NULL,                          -- 'chat' | 'webhook' | 'cron' | 'continuation'
  status        text NOT NULL,                          -- 'received' | 'processed' | 'agent_error' | 'gate_blocked'
  request_json  jsonb NOT NULL,                         -- normalized event payload (the message + metadata)
  response_text text,
  tokens        bigint,
  wall_ms       bigint,
  failure_label text,                                   -- nullable; reason if status='agent_error' or 'gate_blocked'
  gate_outcome  text,                                   -- nullable; 'approved' | 'denied' | 'timeout'
  checkpoint_id text,                                   -- M41 continuation tie-back
  created_at    timestamptz NOT NULL DEFAULT now(),
  completed_at  timestamptz,
  PRIMARY KEY (zombie_id, event_id)
);

CREATE INDEX zombie_events_actor_idx ON core.zombie_events (zombie_id, actor, created_at DESC);
CREATE INDEX zombie_events_workspace_idx ON core.zombie_events (workspace_id, created_at DESC);
```

**Implementation default**: schema slot is the next available number per `schema/embed.zig` ordering. Don't gap-fill.

### §2 — Event envelope

`src/zombie/event_envelope.zig`: a single struct used by every producer (steer, webhook, cron, continuation) and every consumer (worker, history endpoint, SSE).

```
EventEnvelope = {
  event_id: string (Redis stream id),
  zombie_id: uuid,
  workspace_id: uuid,
  actor: string,
  event_type: 'chat' | 'webhook' | 'cron' | 'continuation',
  request: { message: string, metadata: object },
  created_at: timestamptz,
}
```

Encode to Redis stream as flat field/value pairs (Redis convention). Decode on consume.

### §3 — Write path: processEvent INSERT/UPDATE

In `src/zombie/event_loop_helpers.zig::processEvent`:

```
1. Decode EventEnvelope from XREADGROUP message
2. INSERT core.zombie_events (status='received', request_json=envelope.request)
   ON CONFLICT (zombie_id, event_id) DO NOTHING
   (idempotent for replays via XAUTOCLAIM)
3. Run gates: balance, approval. If blocked, UPDATE status='gate_blocked', XACK, return.
4. Resolve secrets, createExecution, startStage (M41 owns these).
5. On stage return:
   UPDATE core.zombie_events
     SET status = exit_ok ? 'processed' : 'agent_error',
         response_text = stage_result.content,
         tokens = stage_result.tokens,
         wall_ms = stage_result.wall_ms,
         checkpoint_id = stage_result.checkpoint_id,  -- nullable
         completed_at = now()
   WHERE zombie_id = $1 AND event_id = $2
6. XACK
```

**Implementation default**: use `RETURNING` clauses for the UPDATE to feed metering (M37's `recordZombieDelivery`).

### §4 — `GET /v1/.../zombies/{id}/events`

Cursor-paginated history. Cursor encodes `(created_at, event_id)`. Filter by `actor` (regex match — e.g., `webhook:*` matches all webhook actors). Default `limit=50`, max `limit=200`.

**Concrete payload example** (`samples/fixtures/m42-event-fixtures/events_response.json`):

```json
{
  "items": [
    {
      "event_id": "1729874000000-0",
      "actor": "steer:kishore",
      "event_type": "chat",
      "status": "processed",
      "request": {"message": "morning health check"},
      "response_text": "All apps healthy. Redis connections at 32% of cap.",
      "tokens": 1840,
      "wall_ms": 8210,
      "created_at": "2026-04-25T08:00:00Z",
      "completed_at": "2026-04-25T08:00:08Z"
    }
  ],
  "next_cursor": "eyJ0IjoiMjAyNi0wNC0yNVQwODowMDowMFoiLCJpZCI6IjE3Mjk4NzQwMDAwMDAtMCJ9"
}
```

### §5 — `GET /v1/.../zombies/{id}/activities/stream` (SSE)

Server-Sent Events tail of `core.zombie_activities` (existing finer-grained log — emits one row per `tool_call_requested`, `tool_call_completed`, `agent_response_chunk`). SSE event types: `tool_call`, `agent_response`, `event_complete`. <200ms latency from row insert to client receive (use `LISTEN/NOTIFY` on the Postgres side; fallback to polling at 100ms if NOTIFY backpressure builds).

**Implementation default**: use `eventsource` / native `EventSource` on the client side, no SDK. Reconnect with `Last-Event-ID` for resume.

### §6 — `zombiectl steer` CLI

Two modes:

- **Batch**: `zombiectl steer {id} "<message>"` → POST `/steer` → poll `GET /events?actor=steer:<user>&limit=1` until `status='processed'` → print `[claw] <response_text>` → exit 0.
- **Interactive**: `zombiectl steer {id}` (no message) → first replay the last 10 events (history readback) → prompt `>` → readline → POST `/steer` → stream incoming via SSE → print `[claw] <chunk>` as chunks arrive → loop. Ctrl-C exits gracefully (doesn't kill zombie).

**Implementation default**: use Node's `readline` module, no external deps. SSE consumed via a thin native fetch wrapper (existing helpers in `zombiectl/src/lib/`).

### §7 — `zombiectl events` CLI

```
zombiectl events {id}                  # last 50 events
zombiectl events {id} --actor=steer    # filter
zombiectl events {id} --actor=webhook:github
zombiectl events {id} --since=2h
zombiectl events {id} --json           # raw JSON output for piping
```

Default print: one line per event with timestamp, actor, status, brief response (first 80 chars). `--json` for full records.

### §8 — Dashboard live panel + events table

`ui/packages/app/src/routes/zombies/[id]/live.tsx`: SSE consumer, renders tool calls + responses as they arrive. Uses existing UI primitives (`<ActivityCard>`, `<ToolCallRow>` — extend if absent). Storybook fixtures from `samples/fixtures/m42-event-fixtures/`.

`ui/packages/app/src/routes/zombies/[id]/events.tsx`: paginated table consuming `GET /events`. Filterable by actor.

### §9 — Idempotency on replay

When a worker crashes mid-event and restarts, M40's XAUTOCLAIM hands the pending event to a new consumer. M42's `INSERT ON CONFLICT DO NOTHING` ensures the event row is created once. The UPDATE is the resumption signal. Test: kill worker mid-tool-call → restart → assert event ends up `processed` with a single row in `core.zombie_events`.

---

## Interfaces

```
HTTP:
  GET  /v1/workspaces/{ws}/zombies/{id}/events?cursor=&actor=&limit=&since=
       → 200 { items: [Event], next_cursor }
  GET  /v1/workspaces/{ws}/zombies/{id}/activities/stream
       → 200 text/event-stream
  POST /v1/workspaces/{ws}/zombies/{id}/steer
       (existing; verified)

CLI:
  zombiectl steer {id} [<msg>]    # interactive or batch
  zombiectl events {id} [flags]   # history print

Redis stream envelope (zombie:{id}:events):
  XADD with fields:
    type:        chat|webhook|cron|continuation
    actor:       steer:<user>|webhook:<source>|cron:<schedule>|continuation:<orig_actor>
    workspace_id: uuid
    request:     <json string>
    created_at:  RFC3339

DB writes:
  INSERT core.zombie_events (..., status='received') at top of processEvent
  UPDATE core.zombie_events SET status, response_text, tokens, wall_ms, completed_at at end
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| `steer` polls but event never reaches 'processed' | Worker died, replay pending | Poll timeout 60s → exit non-zero with "event still in flight, check `zombiectl events`" |
| SSE client disconnects mid-stream | Network blip | Resume via `Last-Event-ID` header on reconnect |
| INSERT conflicts on event_id replay | XAUTOCLAIM redelivery | `ON CONFLICT DO NOTHING` — silent. UPDATE finishes the original row. |
| Event payload exceeds row size | Adversarial input | Reject at API ingress (>1MB request body) before XADD |
| Activity stream backpressure | Slow consumer | Drop oldest; client reconnect uses Last-Event-ID to catch up from history |

---

## Invariants

1. **Every event has exactly one row** in `core.zombie_events` after completion. Replays don't duplicate.
2. **Actor provenance is never null**. Every XADD must include `actor`. Receivers reject malformed events.
3. **Status transitions are monotonic**: `received` → (`processed` | `agent_error` | `gate_blocked`). No going backward.
4. **SSE latency budget**: tool_call_started event in DB → SSE client receives within 200ms p95.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_event_lifecycle_received_processed` | Steer → INSERT (received) → UPDATE (processed) → exactly 1 row, correct timestamps |
| `test_event_replay_idempotent` | Kill worker mid-event → XAUTOCLAIM redelivers → assert 1 row, status=processed |
| `test_actor_filter_steer` | 5 steers + 3 webhooks → `GET /events?actor=steer:*` returns 5 |
| `test_actor_filter_webhook_github` | Webhook events with `actor=webhook:github` filter cleanly |
| `test_steer_cli_batch_roundtrip` | `zombiectl steer {id} "ping"` → exit 0 with `[claw] ...` printed within 60s |
| `test_steer_cli_interactive_history` | `zombiectl steer {id}` (no msg) replays last 10 events before first prompt |
| `test_events_cli_pagination` | 60 events → `events {id}` shows 50 → `events {id} --cursor=<next>` shows 10 |
| `test_sse_tool_call_latency_p95_lt_200ms` | Insert tool_call row → SSE delivers within 200ms p95 over 100 trials |
| `test_sse_resume_with_last_event_id` | Disconnect at event 5 → reconnect with Last-Event-ID=5 → next event 6 arrives |
| `test_event_payload_size_limit` | POST 2MB steer → 413 Payload Too Large; no XADD |

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 10 tests above
- [ ] `zombiectl steer kishore-platform-ops "ping"` returns within 60s with a sensible diagnosis (manual smoke against author's signup)
- [ ] Dashboard `/zombies/{id}/live` shows tool calls streaming in real-time during a steer (manual smoke)
- [ ] `core.zombie_events` retention: forever until M{N+}_001 retention spec defines policy
- [ ] `make memleak` clean
- [ ] `make check-pg-drain` clean
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
