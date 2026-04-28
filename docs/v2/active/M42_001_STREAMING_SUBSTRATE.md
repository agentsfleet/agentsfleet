# M42_001: Streaming Substrate — Unified Event Ingest, History, Steer CLI, Live Watch SSE

**Prototype:** v2.0.0
**Milestone:** M42
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — launch-blocking. The "every event has provenance" + "operator can steer" + "operator can watch live" guarantees converge here. Without this, M37 platform-ops sample's chat experience is broken (no `zombiectl steer`, no event read-back, no live watch).
**Categories:** API, CLI, UI
**Batch:** B1 — parallel with M40, M41, M44, M45.
**Branch:** feat/m42-streaming-substrate
**Depends on:**
- **M40_001** (worker control stream — events flow into per-zombie threads claimed via M40's watcher).
- **M47_001** (Approval Inbox) — *functional* dependency for `gate_blocked` resolution. Until M47 ships, blocked events are stranded; M42 ships a fallback admin endpoint (see §3 step 4 fallback). Independent of M41/M43.
- **Note:** the executor `startStage` progress-callback channel (`tool_call_started` / `agent_response_chunk` / `tool_call_completed`) is implemented in this milestone (originally implied for M41 but not tracked there) — see §3.5.

**Canonical architecture:** `docs/ARCHITECHTURE.md` §8.3 (trigger modes), §9 (steer flow diagram), §10 (event stream + history capability), §12 step 7-9 (event ingest, history INSERT/UPDATE).

---

## Implementing agent — read these first

Before touching any file, read in this order:

1. `docs/ARCHITECHTURE.md` §9 — the canonical A/B/C/D end-to-end sequence (Install / Trigger / Execute / Watch) and the "three durable stores" + "Two streams + one pub/sub channel" tables. Don't reinvent.
2. `src/http/handlers/zombies/steer_integration_test.zig` — current `/steer` endpoint pattern. Note this milestone REWRITES `steer.zig` to direct-XADD; mirror the test scaffolding, not the SET/GETDEL code path.
3. `src/zombie/event_loop.zig` + `src/zombie/event_loop_helpers.zig` — current worker loop; this milestone removes the top-of-loop `GETDEL zombie:{id}:steer` block and rewires `processEvent` to the 13-step write path in §3.
4. `src/queue/redis_client.zig` — Redis client. Pub/sub `SUBSCRIBE` blocks the connection; the SSE handler must hold a dedicated conn outside the request-handler pool.
5. `samples/platform-ops/SKILL.md` — what an actor=steer event's `data` field looks like in practice.
6. `zombiectl/src/commands/zombie.js` + `zombiectl/src/program/routes.js` — existing CLI command + routing pattern; add `steer` + `events` subcommands matching the same shape.

---

## Overview

**Goal (testable):** All event sources land on `zombie:{id}:events` with consistent envelope and `actor` provenance — operator steer, GH webhook, NullClaw cron, M41 continuation (context chunking), M47 continuation (gate-resolved re-enqueue). `core.zombie_events` persists every event start (status='received') and end (status='processed' or 'agent_error') with `actor`, `request_json`, `response_text`, `tokens`, `wall_ms`. Operator runs `zombiectl steer {id} "morning health check"`, sees the zombie's response stream back inline, can Ctrl-C without killing the zombie. Operator runs `zombiectl events {id}` and sees a paginated history with actor filters. Operator opens dashboard `/zombies/{id}/live`, sees tool calls and responses streaming in real-time via SSE with <200ms latency.

**Problem:** Today there is no `core.zombie_events` table. `core.zombie_sessions` holds only the rolling context summary; per-event request/response is lost the moment the next event starts. Debugging means grepping logs. The CLI has no `steer` or `events` subcommands. The dashboard has no live activity stream. The webhook ingest path (M43) has nowhere to write — without M42's stream + history schema, M43 can't land.

**Solution summary:** One new table (`core.zombie_events`), one normalized event envelope, one direct-XADD ingress for steer (no SET/GETDEL key), one write path through `processEvent` that touches three Postgres tables and one Redis pub/sub channel, three HTTP endpoints, two new CLI subcommands, one SSE stream, one dashboard panel. Pre-v2.0.0 → legacy `core.activity_events` table is deleted under the Schema Guard teardown branch (see Files Changed).

- **Schema**:
  - **NEW** `core.zombie_events` with `PRIMARY KEY (zombie_id, event_id)` for idempotency; columns include `actor`, `event_type`, `status`, `request_json`, `response_text`, `tokens`, `wall_ms`, `failure_label`, `checkpoint_id`, `resumes_event_id`, `created_at`, `updated_at` (all epoch-millis `bigint` per project convention). Operator's narrative log (mutable INSERT received → UPDATE terminal).
  - **KEEP** `core.zombie_sessions` — one row per zombie, resume cursor + active execution handle. Worker UPSERTs at start (mark busy) and end (advance bookmark) of each event.
  - **KEEP** `zombie_execution_telemetry` — one row per delivery (UNIQUE `event_id`), immutable billing + latency audit. Joinable to `zombie_events` via `event_id`.
  - **DELETE** `schema/009_core_activity_events.sql` and dependent code — replaced by the Redis pub/sub activity channel below. Pre-v2.0.0 teardown: `rm` the file, drop the `@embedFile` from `schema/embed.zig`, drop the entry in `src/cmd/common.zig`. Slot gap fine.
- **Live tail**: Redis pub/sub channel `zombie:{id}:activity` (NOT a stream, NOT a table). Worker is sole publisher; SSE handler is sole subscriber. Ephemeral, no buffer, no ACK. **No Postgres `LISTEN/NOTIFY`, no Postgres triggers.** If a frame drops, the operator falls back to `GET /events` for the durable record.
- **Steer ingress**: `POST /steer` does direct `XADD zombie:{id}:events` with `actor=steer:<user>`; returns `{event_id}` so callers can correlate. The legacy `zombie:{id}:steer` SET key + worker `GETDEL` poll is deleted in this milestone — see Files Changed.
- **Write path**: `processEvent` in worker INSERTs `zombie_events` (received), UPSERTs `zombie_sessions` (busy), runs the executor with progress callbacks PUBLISHing to `zombie:{id}:activity`, UPDATEs `zombie_events` (processed), INSERTs `zombie_execution_telemetry` (write-once), UPSERTs `zombie_sessions` (idle), PUBLISHes terminal `event_complete`, `XACK`s. Idempotent on event_id replay via M40's XAUTOCLAIM + `ON CONFLICT DO NOTHING`.
- **Read endpoints**: `GET /v1/.../zombies/{id}/events?cursor=&actor=&since=&limit=` (paginated history from `zombie_events`); `GET /v1/workspaces/{ws}/events?cursor=&actor=&zombie_id=&since=&limit=` (workspace-aggregate history — replaces the deleted `workspaces/activity.zig` so the dashboard workspace overview keeps working); `GET /v1/.../zombies/{id}/events/stream` (SSE; SUBSCRIBEs `zombie:{id}:activity` and forwards each PUBLISH as an SSE frame).
- **Executor progress callbacks**: `executor.startStage` grows a callback channel that streams `tool_call_started` / `agent_response_chunk` / `tool_call_completed` from inside the sandbox to the worker. Worker forwards each to `zombie:{id}:activity` via PUBLISH. Originally implied for M41 (Context Layering) but not tracked there; lives in M42 since the live-tail UX is M42's deliverable.
- **CLI**: `zombiectl steer {id} [<msg>]` (interactive REPL or batch); `zombiectl events {id} [--actor=<filter>]` (paginated history print).
- **UI**: dashboard `/zombies/{id}/live` SSE consumer; dashboard `/zombies/{id}/events` paginated table.

---

## Files Changed (blast radius)

### Additions

| File | Action | Why |
|---|---|---|
| `schema/0NN_zombie_events.sql` | NEW | Table definition; next available slot (gap-fill OK after the 009 delete) |
| `schema/embed.zig` | EDIT | Register the new schema file; drop the deleted activity_events `@embedFile` |
| `src/cmd/common.zig` | EDIT | Add `zombie_events` to canonical migration array; drop `activity_events` entry |
| `src/zombie/event_envelope.zig` | NEW | Normalized event envelope: encode/decode for Redis stream + DB |
| `src/zombie/activity_publisher.zig` | NEW | Thin Redis pub/sub helper: `publishActivity(zombie_id, frame)` and helpers for `event_received` / `tool_call_started` / `chunk` / `tool_call_completed` / `event_complete` shapes |
| `src/zombie/event_loop_helpers.zig` | EDIT | `processEvent`: INSERT zombie_events → PUBLISH → UPSERT zombie_sessions (busy) → executor with progress callbacks → UPDATE zombie_events → INSERT zombie_execution_telemetry → UPSERT zombie_sessions (idle) → PUBLISH event_complete → XACK |
| `src/zombie/event_loop.zig` | EDIT | Remove the top-of-loop `GETDEL zombie:{id}:steer` polling block |
| `src/http/handlers/zombies/events.zig` | NEW | `GET /v1/.../zombies/{id}/events` paginated history endpoint |
| `src/http/handlers/zombies/events_stream.zig` | NEW | `GET /v1/.../zombies/{id}/events/stream` SSE endpoint; SUBSCRIBE Redis pub/sub `zombie:{id}:activity` and forward |
| `src/http/handlers/workspaces/events.zig` | NEW | `GET /v1/workspaces/{ws}/events` workspace-aggregate history endpoint; replaces the deleted `workspaces/activity.zig` for the dashboard workspace overview |
| `src/executor/client.zig` | EDIT | Grow `startStage` RPC: add progress callback channel; emit `tool_call_started` / `agent_response_chunk` / `tool_call_completed` frames over the existing Unix-socket RPC |
| `src/executor/progress_callbacks.zig` | NEW | Callback-frame encode/decode + NullClaw tool-call lifecycle hook (start/end taps) and per-token streaming tap |
| `src/http/handlers/zombies/steer.zig` | EDIT | Replace `SET zombie:{id}:steer` with direct `XADD zombie:{id}:events`; return `{event_id}` in response body |
| `src/http/handlers/zombies/events_admin_resume.zig` | NEW | `POST /v1/.../zombies/{id}/events/{event_id}/admin-resume` — fallback for `gate_blocked` rows until M47 ships. Workspace-admin gated, audit-logged, idempotent (409 on already-resumed). Removed when M47 lands. |
| `zombiectl/src/commands/zombie_steer.js` | NEW | `zombiectl steer` subcommand: interactive REPL (SSE-tail) + batch (poll by `event_id`) |
| `zombiectl/src/commands/zombie_events.js` | NEW | `zombiectl events` subcommand: paginated history print |
| `zombiectl/src/program/routes.js` | EDIT | Register `steer` + `events` routes |
| `zombiectl/src/program/command-registry.js` | EDIT | Wire handlers |
| `ui/packages/app/src/routes/zombies/[id]/live.tsx` | NEW | Dashboard live activity panel; on load fetches `GET /events?limit=20`, then opens `GET /events/stream` SSE |
| `ui/packages/app/src/routes/zombies/[id]/events.tsx` | NEW | Dashboard events history table |
| `tests/integration/zombie_events_test.zig` | NEW | E2E: event lifecycle, replay idempotency, actor filter, all three Postgres tables written + correlated via `event_id` |
| `tests/integration/sse_live_watch_test.zig` | NEW | E2E: PUBLISH frame → SSE client receives within 200ms p95; reconnect resumes |
| `samples/fixtures/m42-event-fixtures/` | NEW | Stable JSON payloads used by tests + dashboard storybook |

### Deletions (pre-v2.0.0 Schema Guard teardown branch — `rm` the file, no migration)

| File | Action | Why |
|---|---|---|
| `schema/009_core_activity_events.sql` | DELETE | Replaced by `core.zombie_events` (history) + Redis pub/sub channel (live tail). VERSION=0.30.0 → teardown branch active. |
| `src/zombie/activity_stream.zig` | DELETE | Reads/writes `core.activity_events`. Functionality folded into `zombie_events` + `activity_publisher`. |
| `src/zombie/activity_stream_test.zig` | DELETE | Tests the deleted module. |
| `src/zombie/activity_cursor.zig` | DELETE | Cursor type for `activity_events` paginator. `zombie_events` paginator owns its own cursor in `events.zig`. |
| `src/http/handlers/zombies/activity.zig` | DELETE | Serves `GET /v1/.../zombies/{id}/activity`. Replaced by `events.zig` + `events_stream.zig`. |
| `src/http/handlers/workspaces/activity.zig` | DELETE | Workspace-scoped activity reader for `core.activity_events`. Replaced **in this milestone** by `src/http/handlers/workspaces/events.zig` (see Additions) so the dashboard workspace overview keeps working — no UI regression allowed. |
| `src/queue/constants.zig` | EDIT | Remove `zombie_steer_key_suffix`. |

### Code-level orphan sweep (RULE ORP — must be 0 hits before CHORE(close))

After the deletes, the following symbols must not appear in any non-historical file: `activity_events`, `activity_stream`, `activity_cursor`, `zombie_steer_key_suffix`, `:steer` (Redis-key suffix usage), `GETDEL zombie:`. Run:

```bash
git ls-files | grep -v -E '^docs/|\.md$' | \
  xargs grep -nE 'activity_events|activity_stream|activity_cursor|zombie_steer_key_suffix|GETDEL zombie:|\":steer\"' | head
```

---

## Sections (implementation slices)

### §1 — Schema migration: `core.zombie_events`

```sql
CREATE TABLE core.zombie_events (
  zombie_id        uuid   NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
  event_id         text   NOT NULL,                     -- Redis stream entry id
  workspace_id     uuid   NOT NULL,
  actor            text   NOT NULL,                     -- 'steer:<user>' | 'webhook:<source>' | 'cron:<schedule>' | 'continuation:<original_actor>'
  event_type       text   NOT NULL,                     -- enforced by app-layer enum (no SQL CHECK — see global rule); values: 'chat' | 'webhook' | 'cron' | 'continuation'
  status           text   NOT NULL,                     -- enforced by app-layer enum; values: 'received' | 'processed' | 'agent_error' | 'gate_blocked'
  request_json     jsonb  NOT NULL,                     -- normalized event payload (the message + metadata)
  response_text    text,
  tokens           bigint,
  wall_ms          bigint,
  failure_label    text,                                -- nullable; reason if status='agent_error' or 'gate_blocked'
  checkpoint_id    text,                                -- M41 continuation tie-back
  resumes_event_id text,                                -- nullable; for continuation events, the *immediate* parent event_id
                                                        -- (M41 chunk parent OR M47 gate-blocked parent). Walk the chain to reach origin.
  created_at       bigint NOT NULL,                     -- epoch millis; project convention
  updated_at       bigint NOT NULL,                     -- epoch millis; mutated on every UPDATE (status transition).
                                                        -- For terminal rows ('processed' / 'agent_error' / 'gate_blocked'),
                                                        -- updated_at = the moment the row reached terminal status.
                                                        -- 'gate_blocked' is row-terminal but user-unresolved — query the
                                                        -- continuation chain (resumes_event_id) for the fulfillment.
  PRIMARY KEY (zombie_id, event_id)
);

CREATE INDEX zombie_events_actor_idx ON core.zombie_events (zombie_id, actor, created_at DESC);
CREATE INDEX zombie_events_workspace_idx ON core.zombie_events (workspace_id, created_at DESC);
CREATE INDEX zombie_events_resumes_idx ON core.zombie_events (zombie_id, resumes_event_id) WHERE resumes_event_id IS NOT NULL;
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
  created_at: bigint (epoch millis),
}
```

Encode to Redis stream as flat field/value pairs (Redis convention). Decode on consume.

**Continuation actor — flat with origin tag (rule).** A continuation event's `actor` always references the *original* trigger, never re-nested. A steer that chunks 3 times produces `actor=continuation:steer:kishore` on every continuation, not `continuation:continuation:steer:kishore`. Reasons: single-pass actor-filter regex (`actor LIKE '%steer:kishore'` finds origin + every continuation), bounded length, clean origin chip in UI. Worker enforces: when re-enqueuing a continuation, if the source actor already begins with `continuation:`, reuse it verbatim; otherwise prepend `continuation:`. Tested by `test_continuation_actor_flat`.

**Gate-blocked re-enqueue (rule).** When a gate (M47) approves a previously-blocked event, M47 issues a fresh `XADD zombie:{id}:events` with `actor=continuation:<original_actor>`, `event_type=continuation`, and the new envelope's `resumes_event_id=<blocked_event_id>` (also persisted to the column of the same name on the new `zombie_events` row). The blocked event's `zombie_events` row stays as the historical `gate_blocked` record; the new event is the fulfillment. M42 worker XACKs the original entry on `gate_blocked` (see §3 step 4) — no pending-list pollution, no XAUTOCLAIM thrash, M47 stays decoupled.

**`resumes_event_id` chain rule.** Always points at the *immediate* parent — the most recent event whose status this continuation is resolving. Walk the chain (recursive CTE on the `zombie_events_resumes_idx` index) to reach the origin. Applies symmetrically to M41 chunk continuations and M47 gate-resolved continuations. Tested by `test_resumes_event_id_immediate_parent`.

**Row-terminal vs user-resolved (rule).** `status` carries the lifecycle state; `updated_at` carries the timestamp of the most recent mutation. For terminal status values (`processed` / `agent_error` / `gate_blocked`), `updated_at` is when the row reached terminal. There is no separate "completed_at" / "terminal_at" column — `status + updated_at` is sufficient and matches the project's standard `created_at` / `updated_at` pattern (`core.zombie_sessions`, `core.activity_events`, etc.).

For `gate_blocked` rows, the *row* is terminal but the *user's interaction* is unresolved. Dashboards must NOT render "complete at T" — they must show "blocked at T, awaiting approval." Query for unresolved blocks:

```sql
SELECT * FROM core.zombie_events b
WHERE b.status = 'gate_blocked'
  AND NOT EXISTS (
    SELECT 1 FROM core.zombie_events r
    WHERE r.resumes_event_id = b.event_id
  );
```

This is the operator's "what's stuck waiting on a human" view. Tested by `test_gate_blocked_unresolved_query`.

### §3 — Write path: processEvent (three Postgres tables + Redis pub/sub)

In `src/zombie/event_loop_helpers.zig::processEvent`:

```
 1. Decode EventEnvelope from XREADGROUP message.

 2. INSERT core.zombie_events (status='received', actor, event_type,
    request_json, created_at)
    ON CONFLICT (zombie_id, event_id) DO NOTHING
    (idempotent for replays via XAUTOCLAIM)

 3. PUBLISH zombie:{id}:activity { kind:"event_received", event_id, actor }
    (best-effort; failure does not block the event).

 4. Gates: balance, approval. If blocked →
      UPDATE core.zombie_events SET status='gate_blocked',
                                    failure_label=<gate_name>,
                                    updated_at=now_millis()
      PUBLISH zombie:{id}:activity { kind:"event_complete",
                                      event_id, status:"gate_blocked" }
      XACK zombie:{id}:events  → return.
      (Deterministic exit. The blocked row stays as the historical
       record. When M47 records an approval, it issues a fresh
       XADD zombie:{id}:events with actor=continuation:<original_actor>,
       event_type=continuation,
       resumes_event_id=<blocked_event_id>.
       The new event is a fresh trip through processEvent; the blocked
       row is never mutated to 'processed'.

       FALLBACK (until M47 ships): admin-only endpoint
       POST /v1/.../zombies/{id}/events/{event_id}/admin-resume
       reads the blocked row and issues the same continuation XADD on
       the operator's behalf. Audit-logged via core.audit_log; gated by
       a workspace-admin role. Removed when M47 lands.)

 5. Resolve secrets_map from vault.

 6. UPSERT core.zombie_sessions
      SET execution_id, execution_started_at = now()
    (one row per zombie; mark "busy".)

 7. execution_id := executor.createExecution(workspace_path, {
      network_policy, tools, secrets_map, context })
    (M41 owns the executor.)

 8. stage_result := executor.startStage(execution_id, message)
    For each progress callback the executor RPC streams back:
      tool_call_started   → PUBLISH zombie:{id}:activity
                              { kind:"tool_call_started", name, args_redacted }
      agent_response_chunk → PUBLISH zombie:{id}:activity
                              { kind:"chunk", text }
      tool_call_completed → PUBLISH zombie:{id}:activity
                              { kind:"tool_call_completed", name, ms }

 9. UPDATE core.zombie_events
      SET status = exit_ok ? 'processed' : 'agent_error',
          response_text = stage_result.content,
          tokens        = stage_result.tokens,
          wall_ms       = stage_result.wall_ms,
          checkpoint_id = stage_result.checkpoint_id,
          updated_at    = now_millis()
    WHERE zombie_id = $1 AND event_id = $2;

10. INSERT zombie_execution_telemetry
      (event_id UNIQUE, token_count, time_to_first_token_ms,
       wall_seconds, plan_tier, credit_deducted_cents,
       recorded_at)
    (write-once; immutable. UNIQUE event_id rejects replay.)

11. UPSERT core.zombie_sessions
      SET context_json = { last_event_id, last_response },
          execution_id = NULL,
          checkpoint_at = now()
    (clear "busy" handle; advance bookmark.)

12. PUBLISH zombie:{id}:activity { kind:"event_complete",
                                    event_id, status }

13. XACK zombie:{id}:events.
```

**Implementation defaults**:

- All `PUBLISH` failures are logged + swallowed (best-effort live tail; durable record covers the operator).
- The `RETURNING` clause from the UPDATE in step 9 feeds the telemetry INSERT in step 10 (single round-trip if both run on the same conn — `.drain()` between them per zig-pg-drain rules).
- `args_redacted` strips any `${secrets.x.y}` substituted bytes from the published frame; actual bytes never appear on the activity channel.

### §3.5 — Executor `startStage` progress-callback channel

`executor.startStage` (RPC over the existing Unix socket) grows a callback channel. The worker passes a callback handle when invoking; the executor streams frames over that handle as the NullClaw agent reasons inside the sandbox.

```
executor.startStage(execution_id, message, on_progress) →
  StageResult{ content, tokens, ttft_ms, wall_ms, exit_ok }

  // Inside the sandbox, NullClaw lifecycle hooks invoke on_progress:
  on_progress({ kind: "tool_call_started",   name, args_redacted })
  on_progress({ kind: "agent_response_chunk", text })   // per-token streaming tap
  on_progress({ kind: "tool_call_completed", name, ms })
```

**Implementation defaults**:

- Frames are encoded over the existing executor RPC framing — no new socket. Each frame is a length-prefixed JSON message; the worker reads and dispatches inline before continuing the RPC read loop. The RPC reply is now multiplexed: progress frames interleave with the final `StageResult`.
- **RPC protocol version handshake.** The framing change is wire-incompatible with pre-M42 executors. On socket connect, both sides exchange a `HELLO` frame carrying `{rpc_version: 2}`. Mismatch → both sides log + abort connection with `executor.rpc_version_mismatch` and the worker falls back to refusing to claim that zombie until restart. Pre-v2.0.0 teardown era → no compat shim, no v1 support; fail loud, fail fast. Tested by `test_rpc_version_mismatch_fast_fails`.
- `args_redacted` is built **inside the executor** before the frame leaves the sandbox: any byte range that came from a `secrets_map[NAME][FIELD]` substitution is replaced with `${secrets.NAME.FIELD}` placeholder before the frame is encoded. Resolved secret bytes never cross the RPC boundary on this channel.
  - **Test fixture for byte-stream redaction.** `test_executor_args_redacted_at_sandbox_boundary` wraps the executor-side Unix socket with a recording proxy (`tests/integration/fixtures/rpc_recorder.zig`) that buffers every byte sent. Assertion runs over the captured stream, not the decoded PUBLISH payload. Without the proxy, redaction could be silently bypassed by a buggy encoder.
- `agent_response_chunk` taps NullClaw's per-token streaming output. If NullClaw doesn't expose token streaming today, the executor batches into ≥1 chunk per ~250ms wall window during reasoning.
- **Tool-call heartbeat.** The 250ms chunk guarantee applies only during reasoning. Long tool calls (e.g. 30s `http_request`) emit zero `chunk` frames. To prevent the live-tail UI from looking frozen, the executor emits `tool_call_progress { event_id, name, elapsed_ms }` every ~2s for any tool call still in flight. UI uses these to keep the spinner alive; absence past ~5s is rendered as "stuck." Frame is best-effort like the rest.
- The callback channel is ephemeral — frame loss (e.g. worker stalled, RPC backpressure) does not fail the stage. Dropped frames manifest only as missing live-tail UI events; the durable `zombie_events` row is unaffected.
- Symmetric to the activity-channel rule (Invariant 6): executor unit test feeds a known-secret tool call and asserts the bytes do not appear in any emitted frame.

### §4 — `GET /v1/.../zombies/{id}/events`

Cursor-paginated history. Cursor encodes `(created_at, event_id)`. Filter by `actor` (regex match — e.g., `webhook:*` matches all webhook actors). Default `limit=50`, max `limit=200`.

**`since=` parameter (humanized)**. Accepts:

- **Go-style duration**: `2h`, `30m`, `7d`, `15s` (units: `s`, `m`, `h`, `d`). Server computes `created_after = now() - duration`. Adopted to match `kubectl logs --since`, `docker logs --since`, Prometheus, Loki.
- **RFC 3339 timestamp**: `2026-04-25T08:00:00Z`. Server uses verbatim as `created_after`.
- Anything else → 400 with `error.code = "invalid_since_format"` and the accepted forms in the body.

`since` and `cursor` are mutually exclusive — supplying both → 400 with `error.code = "since_and_cursor_mutually_exclusive"`. The CLI uses `since`; the SSE backfill path uses `cursor`. (`Warning` header was rejected as a half-measure: most HTTP clients drop it silently.)

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
      "created_at": 1745568000000,
      "updated_at": 1745568008210
    }
  ],
  "next_cursor": "eyJ0IjoiMjAyNi0wNC0yNVQwODowMDowMFoiLCJpZCI6IjE3Mjk4NzQwMDAwMDAtMCJ9"
}
```

### §4b — `GET /v1/workspaces/{ws}/events` (workspace aggregate)

Replaces the deleted `workspaces/activity.zig`. Same shape as §4, scoped to workspace, with optional `zombie_id` filter to drill down. Reads `core.zombie_events` filtered by `workspace_id` (RLS-protected). Default `limit=50`, max `limit=200`. Same `since=` rules as §4.

```
GET /v1/workspaces/{ws}/events?cursor=&actor=&zombie_id=&since=&limit=
  → 200 { items: [Event + zombie_id], next_cursor }
```

Items carry an extra `zombie_id` field so the dashboard workspace overview can group/colour by zombie. The dashboard workspace activity panel re-points to this endpoint.

**Acceptance**: dashboard workspace overview must render at least one row per active zombie within the workspace, identical visual contract to the pre-deletion `activity.zig` view.

### §5 — `GET /v1/.../zombies/{id}/events/stream` (SSE)

Server-Sent Events tail of the **Redis pub/sub channel `zombie:{id}:activity`**. The handler:

1. Verifies workspace + zombie ownership (same gate as `/events`).
2. Sets `Content-Type: text/event-stream`, disables compression + buffering.
3. Issues `SUBSCRIBE zombie:{id}:activity` on a dedicated Redis connection (pub/sub blocks the conn — must NOT share with the request-handler pool).
4. For each PUBLISH frame, writes one SSE event:
   ```
   id:<sequence>\nevent:<kind>\ndata:<json payload>\n\n
   ```
5. On client disconnect: `UNSUBSCRIBE`, return the connection to the pub/sub pool, close.

**No Postgres `LISTEN/NOTIFY`. No Postgres triggers. No polling.** Latency is the Redis round-trip — well under the 200ms p95 budget. If a frame drops (slow consumer, network blip), the operator pulls the gap from `GET /events` for the durable record.

**Frame kinds the SSE forwards** (one-to-one with the worker's PUBLISH calls in §3):

| `event:` line | `data:` payload | When |
|---|---|---|
| `event_received` | `{event_id, actor}` | Step 3 of processEvent |
| `tool_call_started` | `{event_id, name, args_redacted}` | Executor progress callback |
| `tool_call_progress` | `{event_id, name, elapsed_ms}` | Executor heartbeat (~2s) while tool call in flight |
| `chunk` | `{event_id, text}` | Executor progress callback |
| `tool_call_completed` | `{event_id, name, ms}` | Executor progress callback |
| `event_complete` | `{event_id, status}` | Step 12 of processEvent |

**Reconnect semantics**: pub/sub has no replay. The `id:` line carries a server-side monotonic sequence so the client knows whether it missed a frame; on reconnect the client SHOULD call `GET /events?since=<event_id_of_last_seen>&limit=20` to backfill the gap from the durable log, then keep the new SSE connection.

**Implementation default**: use native `EventSource` on the client side, no SDK.

**Auth (dual-accept, strict no-fallthrough)**. The browser `EventSource` API cannot set custom request headers — only cookies and standard browser-managed headers cross the wire. The endpoint therefore accepts **either**:

- **Session cookie** (browser dashboard path). The dashboard's existing session cookie is sent automatically by `EventSource`. Server validates cookie → workspace + user → ACL gate identical to `/events`.
- **`Authorization: Bearer <api_key>`** (CLI / programmatic path). `zombiectl` runs in Node, can set arbitrary headers via the `eventsource` npm package or a `fetch`-based SSE consumer. Same workspace API key as the rest of the `/v1/...` surface.

**Strict resolution order — no fall-through on validation failure** (timing-oracle prevention + leaked-cookie defense):

```
if request has Cookie header:
    validate cookie → on failure, return 401 (do NOT also try Authorization)
elif request has Authorization header:
    validate Bearer → on failure, return 401
else:
    return 401
```

A stale/leaked cookie does not silently fall through to a valid Bearer; the request is 401'd. No query-param tokens (avoids leaking long-lived API keys via URL/referrer/access logs).

Tests: `test_sse_auth_cookie_browser_path`, `test_sse_auth_bearer_cli_path`, `test_sse_auth_neither_401`, `test_sse_auth_stale_cookie_does_not_fallthrough_to_bearer` (request with invalid cookie + valid Bearer → 401, NOT 200).

**Reconnect / sequence ID source**. The `id:` line on each SSE frame is a **per-connection, in-memory monotonic counter** that resets to 0 on each new SUBSCRIBE. The server **ignores the `Last-Event-ID` request header** — sequence IDs are not durable and have no cross-connection meaning. Clients MUST backfill via `GET /events?cursor=<last_seen_event_id>` after reconnect; the new SSE then resumes from sequence 0. Documented to avoid the trap of clients trusting `Last-Event-ID` to resume a stream that never persisted past the previous SUBSCRIBE.

### §6 — `zombiectl steer` CLI

Two modes, both correlated by the `event_id` returned from `POST /steer`:

- **Batch**: `zombiectl steer {id} "<message>"` →
  1. `POST /steer` → captures `{event_id}` from response body.
  2. Opens `GET /events/stream` SSE; filters on the captured `event_id`.
  3. Prints `[claw] <chunk>` as `chunk` frames arrive.
  4. Closes when `event_complete` frame for this `event_id` arrives. Exit 0 on `status=processed`, non-zero on `agent_error`.
  5. If SSE disconnects, falls back to polling `GET /events?since=<event_id>&limit=1` until `status` is terminal (60s timeout).

- **Interactive**: `zombiectl steer {id}` (no message) →
  1. `GET /events?limit=10` to print recent history.
  2. Opens persistent `GET /events/stream` SSE.
  3. `readline >` prompt → on each line: `POST /steer` → tracks the new `event_id` → SSE chunks tagged with that id print as `[claw] <chunk>`. Returns to `>` after `event_complete`.
  4. Ctrl-C closes the SSE connection and exits — the zombie keeps running.

**Implementation default**: Node's `readline` + a thin native `fetch` SSE consumer in `zombiectl/src/lib/sse.js`. No external deps.

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
  POST /v1/workspaces/{ws}/zombies/{id}/steer
       body: { message }
       → 202 { event_id }                       (direct XADD; returned id
                                                 used by CLI for correlation)
  GET  /v1/workspaces/{ws}/zombies/{id}/events?cursor=&actor=&limit=&since=
       → 200 { items: [Event], next_cursor }
  GET  /v1/workspaces/{ws}/zombies/{id}/events/stream
       → 200 text/event-stream
       (SSE; SUBSCRIBEs zombie:{id}:activity)

CLI:
  zombiectl steer {id} [<msg>]    # interactive (REPL) or batch (correlated by event_id)
  zombiectl events {id} [flags]   # history print

Redis surfaces:
  Stream zombie:{id}:events  (group zombie_workers — durable, at-least-once)
    XADD fields:
      type:        chat|webhook|cron|continuation
      actor:       steer:<user>|webhook:<source>|cron:<schedule>|continuation:<orig_actor>
      workspace_id: uuid
      request:     <json string>
      created_at:  RFC3339

  Pub/sub channel zombie:{id}:activity  (no group, ephemeral)
    PUBLISH frames:
      { kind:"event_received",     event_id, actor }
      { kind:"tool_call_started",  event_id, name, args_redacted }
      { kind:"chunk",              event_id, text }
      { kind:"tool_call_completed",event_id, name, ms }
      { kind:"event_complete",     event_id, status }

Postgres writes (per event):
  INSERT core.zombie_events            (status='received', created_at, updated_at) → step 2
  UPDATE core.zombie_events            (status, response_text, updated_at)         → step 9
  INSERT zombie_execution_telemetry    (UNIQUE event_id; immutable)         → step 10
  UPSERT core.zombie_sessions          (busy=execution_id; idle=NULL)        → steps 6, 11
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| `steer` SSE drops; event never reaches `event_complete` | Worker died mid-event, XAUTOCLAIM pending | CLI falls back to polling `GET /events?since=<event_id>&limit=1` for 60s, then exits non-zero with "event still in flight, check `zombiectl events`" |
| SSE client disconnects mid-stream | Network blip, slow consumer | Client reconnects, calls `GET /events?since=<last_event_id>&limit=20` for durable backfill, then re-opens SSE. Pub/sub has no replay. |
| INSERT conflicts on `event_id` replay | XAUTOCLAIM redelivery (M40) | `ON CONFLICT (zombie_id, event_id) DO NOTHING` on `zombie_events`; UNIQUE-violation on `zombie_execution_telemetry` is silently ignored if the row already exists (idempotent on resume). UPDATE in step 9 finishes the original `zombie_events` row. |
| Event payload exceeds size limit | Adversarial input | Reject at API ingress (>1MB request body) before XADD; return 413. |
| Activity pub/sub backpressure | Slow SSE subscriber | Redis drops messages to that subscriber. Subscriber reconnect path (above) backfills from `zombie_events`. Other subscribers + the durable record are unaffected. |
| `PUBLISH` fails (Redis down) | Redis outage | Log + swallow; durable path (`zombie_events` + `zombie_execution_telemetry` + `zombie_sessions`) continues. Operator loses live tail; gets full record on next `GET /events`. |

---

## Invariants

1. **Single ingress**. The only Redis surface receiving operator/webhook/cron/continuation events is the stream `zombie:{id}:events`. No SET/GETDEL key, no parallel "steer" stream.
2. **Three Postgres rows per processed event, one join key**. Every successfully processed event ends with exactly one new row in `core.zombie_events` (mutable, status=`processed`), one new row in `zombie_execution_telemetry` (immutable), and one mutated row in `core.zombie_sessions`. All three reference the same `event_id`. Replays never duplicate any of them.
3. **Actor provenance is never null**. Every XADD must include `actor`. Receivers reject malformed events.
4. **Status transitions are monotonic AND terminal**: `received` → (`processed` | `agent_error` | `gate_blocked`). All three are terminal — no row is ever mutated after reaching one of them. `gate_blocked` rows are never reopened; M47 issues a *fresh* `XADD` with `actor=continuation:<original_actor>` and `metadata.resumes_event_id=<blocked_id>`, producing a new `zombie_events` row whose lifecycle is independent.
5. **SSE latency budget**: PUBLISH on `zombie:{id}:activity` → SSE client receives the corresponding frame within 200ms p95.
6. **Pub/sub never leaks secrets**. The `args_redacted` field strips substituted `${secrets.x.y}` bytes before PUBLISH. Audited by a unit test that feeds a known-secret arg into a synthetic tool call and asserts the bytes do not appear in any frame.
7. **Durable record is independent of pub/sub**. `PUBLISH` failure does not roll back any Postgres write or block XACK.

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_event_lifecycle_three_tables` | Steer → INSERT zombie_events (received) → UPDATE (processed) → INSERT zombie_execution_telemetry → UPSERT zombie_sessions (idle) → assert 1 row each, all keyed by the same `event_id`, correct timestamps |
| `test_steer_returns_event_id` | `POST /steer` body must include `event_id` matching the XADD result |
| `test_event_replay_idempotent` | Kill worker mid-event → XAUTOCLAIM redelivers → assert 1 row in zombie_events, 1 row in telemetry, status=processed |
| `test_actor_filter_steer` | 5 steers + 3 webhooks → `GET /events?actor=steer:*` returns 5 |
| `test_actor_filter_webhook_github` | Webhook events with `actor=webhook:github` filter cleanly |
| `test_no_legacy_steer_key` | `POST /steer` does NOT touch `zombie:{id}:steer`. Redis MONITOR during a test steer must show XADD on `zombie:{id}:events` and zero SET/GETDEL on `:steer` |
| `test_steer_cli_batch_roundtrip` | `zombiectl steer {id} "ping"` correlates by `event_id` → exit 0 with `[claw] ...` printed within 60s |
| `test_steer_cli_interactive_history` | `zombiectl steer {id}` (no msg) prints last 10 events from `GET /events?limit=10` before first prompt |
| `test_events_cli_pagination` | 60 events → `events {id}` shows 50 → `events {id} --cursor=<next>` shows 10 |
| `test_sse_publish_latency_p95_lt_200ms` | Worker `PUBLISH zombie:{id}:activity` → SSE client receives matching frame within 200ms p95 over 100 trials |
| `test_sse_reconnect_backfills_via_events` | Disconnect SSE mid-event → client reconnects via `GET /events?since=<id>` + new SSE → no missing event in operator-visible state |
| `test_pubsub_failure_does_not_block` | Force Redis pub/sub to error → assert worker still completes the event (zombie_events processed, telemetry inserted, session idle, XACKed) |
| `test_args_redacted_no_secret_leak` | Tool call with `${secrets.fly.api_token}` → assert no frame on `zombie:{id}:activity` contains the resolved secret bytes |
| `test_event_payload_size_limit` | POST 2MB steer → 413 Payload Too Large; no XADD |
| `test_orphan_sweep_no_legacy_symbols` | Repository-wide grep for `activity_events`, `activity_stream`, `activity_cursor`, `zombie_steer_key_suffix`, `:steer` constant, `GETDEL zombie:` returns 0 hits in non-historical files |
| `test_gate_blocked_xacked_immediately` | Force balance gate to fail → assert `zombie_events.status='gate_blocked'`, row XACKed (zero entries in pending list for that consumer), no XAUTOCLAIM redelivery |
| `test_gate_resolved_continuation_event` | Synthesise M47 re-enqueue (`XADD actor=continuation:steer:foo metadata.resumes_event_id=<blocked>`) → new event processes to `processed`, original row stays `gate_blocked` (immutable) |
| `test_continuation_actor_flat` | Origin `steer:kishore` chunks 10 times → all 10 continuation events carry `actor=continuation:steer:kishore`, never `continuation:continuation:...`. Repeat with origin `webhook:github` and `cron:0_*/30_*_*_*` — three actor families, prefix-detection holds at depth 10 each. |
| `test_resumes_event_id_immediate_parent` | Chain A → B (M41 chunk) → C (gate-blocked) → D (M47 resumed). Assert `B.resumes_event_id=A`, `C.resumes_event_id=B`, `D.resumes_event_id=C`. Recursive CTE walk from D returns full chain in order. |
| `test_gate_blocked_unresolved_query` | 5 blocked rows, 2 already resumed via continuation, 3 still waiting → operator's "unresolved blocks" query returns exactly the 3. |
| `test_admin_resume_fallback_endpoint` | Until M47 ships: `POST /events/{id}/admin-resume` on a `gate_blocked` event by a workspace admin → fresh continuation XADD, audit-logged. Non-admin → 403. Already-resumed event → 409. |
| `test_rpc_version_mismatch_fast_fails` | Old worker (rpc_version=1) connects to new executor (rpc_version=2) → both sides log `executor.rpc_version_mismatch` and abort connection within 100ms. No partial frames decoded. |
| `test_tool_call_progress_heartbeat` | Synthetic 6-second tool call → SSE client receives ≥3 `tool_call_progress` frames at ~2s intervals. Absence past 5s renders "stuck" in UI fixture. |
| `test_workspace_events_rls_no_cross_tenant` | Workspace A's API key calls `GET /v1/workspaces/{B}/events` → 403 (or 404 per project's IDOR convention). Workspace A's events are never returned in workspace B's response. |
| `test_since_and_cursor_mutually_exclusive_400` | `GET /events?since=2h&cursor=<id>` → 400 with `error.code=since_and_cursor_mutually_exclusive`. |
| `test_sse_auth_stale_cookie_does_not_fallthrough_to_bearer` | Request with an invalid cookie + valid Bearer → 401 (proves no fall-through); request with no cookie + valid Bearer → 200. |
| `test_sse_sequence_resets_on_reconnect` | Open SSE, receive frames id=0..N, disconnect, reconnect → first frame on new connection is id=0. Server ignores `Last-Event-ID` request header. |
| `test_executor_progress_callbacks_emit` | `executor.startStage` with a synthetic NullClaw run that calls 2 tools + emits 5 token chunks → worker receives 2 `tool_call_started`, 2 `tool_call_completed`, 5 `agent_response_chunk` callbacks in order |
| `test_executor_args_redacted_at_sandbox_boundary` | Tool call uses `${secrets.fly.api_token}` → frame leaving the executor RPC contains placeholder, never resolved bytes (asserted at the RPC byte stream, not just the PUBLISH payload) |
| `test_since_param_duration` | `GET /events?since=2h` → server filters by `created_after = now() - 2h`. Forms `30m`, `7d`, `15s` accepted |
| `test_since_param_rfc3339` | `GET /events?since=2026-04-25T08:00:00Z` → server filters by literal timestamp |
| `test_since_param_invalid_400` | `GET /events?since=bogus` → 400 with `error.code=invalid_since_format` |
| `test_workspace_events_endpoint` | 3 zombies in workspace, 5 events each → `GET /v1/workspaces/{ws}/events` returns 15 sorted by `created_at DESC`; `?zombie_id=X` filter narrows to 5 |
| `test_workspace_events_replaces_activity_ui` | Dashboard workspace overview renders at least one row per active zombie via the new endpoint — no UI regression vs. the deleted `activity.zig` view |
| `test_sse_auth_cookie_browser_path` | SSE request with valid session cookie, no `Authorization` header → 200 + SUBSCRIBE issued |
| `test_sse_auth_bearer_cli_path` | SSE request with `Authorization: Bearer <api_key>`, no cookie → 200 + SUBSCRIBE issued |
| `test_sse_auth_neither_401` | SSE request with neither → 401, no SUBSCRIBE issued |

---

## Acceptance Criteria

- [ ] `make test-integration` passes the full test list above (originally 15; amendments add 22 → 37 total)
- [x] Schema follows project convention: epoch-millis `bigint` for `created_at` / `updated_at`, no `timestamptz`, no SQL CHECK constraints (app-layer enums for `status` and `event_type`)
- [x] `gate_blocked` rows have `updated_at` set, `response_text` NULL, and `failure_label` populated; dashboard renders them as "blocked, awaiting approval" not "completed"
- [ ] Admin-resume fallback endpoint removed in the same PR that lands M47_001 (tracked as a M47 follow-up TODO)
- [ ] Dashboard workspace overview shows live activity per zombie via the new `/v1/workspaces/{ws}/events` endpoint — verified manually before CHORE(close), no regression vs. the deleted `activity.zig` view
- [ ] Executor RPC progress-callback channel emits the three frame kinds; redaction proven at the RPC byte stream, not just the PUBLISH payload
- [ ] `zombiectl steer kishore-platform-ops "ping"` returns within 60s with a sensible diagnosis (manual smoke against author's signup)
- [ ] Dashboard `/zombies/{id}/live` shows tool calls streaming in real-time during a steer (manual smoke)
- [x] Legacy table + module deletes are atomic at branch merge (the wire-format substrate switchover passes through intermediate commits as a deliberate slice ordering, but the merge to main is what's atomic). Slot 009 may be reused or left as a gap; do not gap-fill with a placeholder file.
- [x] Orphan sweep (RULE ORP) returns 0 hits for the deleted symbols above before CHORE(close).
- [ ] `core.zombie_events` retention: forever until M{N+}_001 retention spec defines policy.
- [ ] `make memleak` clean.
- [x] `make check-pg-drain` clean (every `conn.query()` followed by `.drain()` in `processEvent` and the `/events` handler).
- [x] Cross-compile clean: x86_64-linux + aarch64-linux.
