---
Milestone: M10
Workstream: M10_001
Name: PIPELINE_V1_REMOVAL
Status: DONE
Priority: P1 — removes dead v1 pipeline worker; unblocks zombie-only worker binary
Categories: API, CLI, ZIG
Branch: feat/m10-pipeline-v1-removal
Created: Apr 10, 2026
---

# M10_001 — Pipeline v1 Removal

## Goal

Remove the v1 pipeline worker (GitHub PR-solver) and replace its streaming
capability with a v2 zombie-native SSE stream and chat-inject API. After this
milestone, the worker binary runs only zombie threads. The `src/pipeline/`
directory is deleted. Operators can watch zombie activity in real-time via SSE
and steer a running zombie by posting a chat message.

**Demo:** `zombiectl logs --stream <zombie-id>` tails live SSE output as a
zombie processes an inbound webhook event. `zombiectl chat <zombie-id> "check
the latest PR"` injects a message and the zombie picks it up on its next poll.

---

## Workstreams

| ID | Name | Batch |
|---|---|---|
| M10_001 | Pipeline v1 removal | B1 |
| M10_002 | Zombie SSE stream + chat-inject | B1 |

B1 runs concurrently: both workstreams touch non-overlapping files.
Gate: both must pass `make lint && make test` before merge.

---

## Surface Area Checklist

- [x] **OpenAPI spec update** — yes: new `GET /v1/zombies/{id}/stream` (SSE) and `POST /v1/zombies/{id}/message`. Remove `/v1/runs/*` endpoints once pipeline tables are confirmed unused.
- [x] **`zombiectl` CLI changes** — yes: `logs --stream` flag and `chat` subcommand. CLI surface approval required before M10_002 ships.
- [x] **User-facing doc changes** — yes: remove "runs" from docs, add zombie stream + chat-inject pages.
- [x] **Release notes** — minor bump: `0.7.0` → `0.8.0` (feature: zombie SSE + v1 removal).
- [x] **Schema changes** — pipeline tables (`runs`, `gate_results`, `proposals`, ...) to be dropped. Each DROP in its own SQL file ≤100 lines. Schema drop gated behind `ZOMBIE_V2_ONLY=true` env flag until confirmed no rollback needed.

---

# M10_001 — Pipeline v1 Removal (Workstream)

## Scope

Strip the v1 GitHub PR-solver from the worker binary and delete the pipeline
source tree. The executor sidecar and zombie event loop are untouched.

### What is the pipeline worker?

`src/pipeline/` contains the v1 GitHub-native agent: it claims runs from a
Postgres job queue, checks out repos into sandboxes, runs gate loops (lint/test/
build), and opens PRs. It predates Zombies. It is imported by:

- `src/cmd/worker.zig` — spawns `pipeline_cfg` + `workerLoop` threads
- `src/cmd/serve.zig` — imports `pipeline/worker.zig` (one reference, used for
  WorkerState type only — can be inlined or removed)

The pipeline worker is 43 files, ~6000 lines. The v1 runs HTTP endpoints
(`/v1/runs/*`) serve pipeline run state. These endpoints are NOT deleted in
this workstream — they are gated behind a deprecation flag and removed in M10_003
after a 30-day operator grace period (see §Schema below).

---

## Section 1: Audit and Gate

### 1.1 — Runs endpoint usage audit
Confirm `GET /v1/runs`, `POST /v1/runs`, `GET /v1/runs/{id}/stream` are not
called by any active workspace in the last 30 days. Query:
```sql
SELECT endpoint, COUNT(*) FROM core.request_log
WHERE endpoint LIKE '/v1/runs%' AND created_at > NOW() - INTERVAL '30 days'
GROUP BY endpoint;
```
If any hits: flag to operator, set `PIPELINE_DEPRECATION_DATE`, proceed anyway
(deletion gated on M10_003 grace period, not this workstream).

### 1.2 — Identify dead code boundary
`grep -r "pipeline\|WorkerState\|workerLoop\|GateLoop" src/cmd/ src/http/` to
enumerate every import outside `src/pipeline/` itself.
Expected: `worker.zig`, `serve.zig` only.

### 1.3 — Schema inventory
List pipeline-owned tables:
```sql
SELECT tablename FROM pg_tables
WHERE schemaname = 'core'
AND tablename IN ('runs','gate_results','proposals','run_locks','run_credits',
                  'run_artifacts','sandbox_events');
```
Each table gets its own `DROP TABLE IF EXISTS` SQL file in `schema/drops/`.
No ALTER or DROP executed in this workstream — files only.

### 1.4 — Compile-time boundary check
`zig build -Dtarget=x86_64-linux 2>&1 | grep pipeline` must be zero output after
all pipeline imports are removed from `cmd/`. This is the green gate for §2.

---

## Section 2: Strip Pipeline from Worker Binary

### 2.1 — Remove pipeline thread spawning from `worker.zig`
Delete the `pipeline_cfg` block (lines ~194–211) and the `worker_threads` spawn
loop (lines ~209–212). Keep zombie thread spawning, signal watcher, event bus.
Resulting `worker.zig` should be under 180 lines.

### 2.2 — Remove pipeline import from `serve.zig`
`serve.zig` imports `pipeline/worker.zig` only for `WorkerState`. Replace with
`null` or inline the 3-field struct directly. Remove the import line.

### 2.3 — Remove v1 env var enforcement
`env_vars.zig` enforces `DATABASE_URL_WORKER` and GitHub App credentials for
`.worker` mode. GitHub App vars (`GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`) are
pipeline-only. Remove them from the `.worker` enforcement list. Keep DB + Redis.

### 2.4 — Delete `src/pipeline/`
`trash src/pipeline/` (43 files). Update `build.zig` to remove the pipeline
module. Confirm `make build` passes with no pipeline references.

---

## Section 3: Runs Endpoint Deprecation (non-breaking)

### 3.1 — Add `Deprecation` header to `/v1/runs/*`
All runs handlers return `Deprecation: true` and
`Sunset: <date 30 days from now>` headers (RFC 7234 Warning equivalent).
No functional change — endpoints still work.

### 3.2 — Log deprecation hits
Each request to `/v1/runs/*` writes one row to `core.deprecation_log`
`(endpoint, workspace_id, created_at)`. Used to audit §1.1 in M10_003.

### 3.3 — `PIPELINE_V1_DISABLED` env flag
When `PIPELINE_V1_DISABLED=true`, the runs endpoints return 410 Gone with
error body `{"error":"pipeline_v1_removed","sunset":"<date>"}`. Default: false.
Operators set this when they have confirmed no active runs clients.

### 3.4 — Migration guide in docs
`docs/migration/pipeline-v1-removal.md`: what changed, how to move to zombie
event stream, sunset date for `/v1/runs/*` endpoints.

---

## Acceptance Criteria

1. `make build` passes with no `pipeline` imports or references in `src/cmd/`.
2. `make test` passes (all pipeline tests deleted with the directory).
3. `make lint` passes — no dead imports.
4. Cross-compile: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` both pass.
5. `worker.zig` < 180 lines.
6. `/v1/runs/health` returns 200 with `Deprecation: true` header.
7. `PIPELINE_V1_DISABLED=true` → runs endpoints return 410.

---

## Error Contracts

| Condition | Code | HTTP |
|---|---|---|
| Runs endpoint with `PIPELINE_V1_DISABLED=true` | `UZ-RUNS-410` | 410 |
| Pipeline env var missing (pre-removal) | `UZ-STARTUP-002` | — (exit 1) |

---

# M10_002 — Zombie SSE Stream + Chat-Inject (Workstream)

## Scope

Wire the v1 SSE pub/sub infrastructure to the zombie event loop. Add two new
endpoints: `GET /v1/zombies/{id}/stream` (SSE activity tail) and
`POST /v1/zombies/{id}/message` (inject a chat message into the zombie's event
queue).

### Why this is not a big lift

V1 already has:
- `redis_client.publish(channel, data)` — `src/queue/redis_client.zig:57`
- `queue_pubsub.Subscriber` — subscribe to a channel, read messages with
  heartbeat timeout (`PUBSUB_READ_TIMEOUT_MS = 25s`)
- `stream.zig` pattern — subscribe, emit SSE chunks, replay from DB, heartbeat

The only gap: `event_loop.zig` → `processEvent()` result goes only to Postgres
(`activity_stream.logEvent`). One `redis.publish()` call wires the stream.
The HTTP handler is ~150 lines — 80% identical to `stream.zig`.

---

## Section 1: Publish zombie activity to Redis pub/sub

### 1.1 — Add publish call in `event_loop.zig`
After `processEvent()` returns (line ~183), publish the activity to Redis:
```zig
const channel = try std.fmt.allocPrint(alloc, "zombie:{s}:stream", .{session.zombie_id});
defer alloc.free(channel);
const payload = try std.fmt.allocPrint(alloc,
    \\{{"event_type":"{s}","detail":"{s}","created_at":{d}}}
, .{ event_type, detail, std.time.milliTimestamp() });
defer alloc.free(payload);
cfg.redis.publish(channel, payload) catch |err|
    log.warn("zombie_event_loop.pubsub_fail zombie_id={s} err={s}", .{ session.zombie_id, @errorName(err) });
```
Publish is fire-and-forget — never blocks the event loop.

### 1.2 — Unit test: publish is called on successful event
Inject a mock redis client, process a synthetic event, assert `publish` was
called once with channel `zombie:{id}:stream` and a parseable JSON payload.

### 1.3 — Unit test: publish failure does not crash the event loop
Mock redis returns error on `publish`. Verify the loop continues and processes
the next event without error.

### 1.4 — Integration test: end-to-end pub/sub round-trip
Send a synthetic event to `zombie:{id}:events` Redis stream, run the event
loop for one iteration (real Redis in test env via `make up`), assert the
pub/sub channel `zombie:{id}:stream` received one message.

---

## Section 2: SSE stream handler (`zombie_stream_api.zig`)

New file: `src/http/handlers/zombie_stream_api.zig`

### 2.1 — `GET /v1/zombies/{id}/stream`
Authenticate + authorize (same pattern as `zombie_api.zig`). Check zombie
exists in `core.zombies`. Set SSE headers. If zombie status is `killed` or
`stopped`, replay stored activity from `core.activity_events` and close.
Subscribe to `zombie:{id}:stream` pub/sub. Emit SSE events as:
```
id: {created_at_ms}
event: zombie_activity
data: {"event_type":"...", "detail":"...", "created_at":...}

```
Heartbeat every 20s (`": heartbeat\n\n"`). Never terminates unless zombie
status becomes `killed`/`stopped` (checked in the pub/sub read loop via DB
query, same as `stream.zig` terminal state check).

### 2.2 — Reconnect replay from `core.activity_events`
Support `Last-Event-ID` header. If provided, replay events from
`core.activity_events` where `created_at > last_event_id`. Uses
`activity_stream.queryByZombie()` with cursor. Same pattern as
`streamStoredEvents()` in `stream.zig`.

### 2.3 — Poll fallback
If Redis pub/sub connect fails, fall back to polling `core.activity_events`
every 2s (same as `streamViaPoll` in `stream.zig`). Log the degradation.

### 2.4 — File must be ≤400 lines
Split into `zombie_stream_api.zig` + `zombie_stream_helpers.zig` if needed.

---

## Section 3: Chat-inject endpoint

### 3.1 — `xaddZombieMessage` in `redis_zombie.zig`
```zig
pub fn xaddZombieMessage(
    client: *redis_client.Client,
    zombie_id: []const u8,
    message: []const u8,
) ![]const u8 // returns message_id
```
Issues `XADD zombie:{id}:events * event_type user_message payload <message>`.
The event loop's existing `xreadgroupZombie` will pick this up on the next poll
(≤`zombie_xread_block_ms` latency).

### 3.2 — `POST /v1/zombies/{id}/message`
Request body: `{"message": "..."}`. Max 4096 bytes (same limit as interrupt.zig).
Authenticate + authorize. Validate zombie exists and is `active`. Call
`xaddZombieMessage`. Return `{"message_id": "...","queued_at": <ms>}`.
Error if zombie is `killed`/`stopped` → 409 with `UZ-ZMB-INACTIVE`.

### 3.3 — Unit test: xaddZombieMessage happy path
Mock redis client, verify XADD command args match the expected format.
Verify returned message_id is a non-empty string.

### 3.4 — Integration test: injected message reaches event loop
`POST /v1/zombies/{id}/message` with a message, then verify the event loop
consumed it (check `core.activity_events` for `event_type=user_message`).

---

## Section 4: Router wiring

### 4.1 — Register new routes in `handler.zig`
```zig
// In handler.zig, alongside existing zombie routes:
pub const handleStreamZombie = zombie_stream_api_http.handleStreamZombie;
pub const handleZombieMessage = zombie_stream_api_http.handleZombieMessage;
```
And in the httpz router:
```
GET  /v1/zombies/:id/stream   → handleStreamZombie
POST /v1/zombies/:id/message  → handleZombieMessage
```

### 4.2 — Auth + RBAC aligned with existing zombie handlers
Both endpoints use the same `common.authenticate` + workspace RBAC check as
`handleDeleteZombie`. No new RBAC roles needed.

### 4.3 — OpenAPI spec updated
`openapi.json` gains two new paths. Include SSE response schema
(`text/event-stream`) and message request body schema.

### 4.4 — `make lint` and `make test-integration` pass

---

## Acceptance Criteria

1. `curl -N -H "Authorization: Bearer <token>" /v1/zombies/{id}/stream` returns
   `Content-Type: text/event-stream` and emits `zombie_activity` events as the
   zombie processes its next inbound event.
2. `POST /v1/zombies/{id}/message {"message":"hello"}` returns 200 with a
   `message_id`. Within ≤3s (next XREADGROUP poll), the zombie event loop logs
   `event_type=user_message` in `core.activity_events`.
3. Reconnect with `Last-Event-ID: <ts>` replays only events after that timestamp.
4. `": heartbeat"` emitted within 25s on an idle stream.
5. Posting to a `killed` zombie returns 409 `UZ-ZMB-INACTIVE`.
6. All new files ≤400 lines.
7. `make test-integration` passes (requires `make up`).

---

## Error Contracts

| Condition | Code | HTTP |
|---|---|---|
| Zombie not found | `UZ-ZMB-001` | 404 |
| Zombie not active (stopped/killed) | `UZ-ZMB-INACTIVE` | 409 |
| Message too long (>4096 bytes) | `UZ-ZMB-MSG-TOO-LONG` | 400 |
| Redis unavailable (stream endpoint) | — | falls back to poll, no error |
| Redis unavailable (chat-inject) | `UZ-QUEUE-001` | 503 |

---

## Interfaces

### New: `zombie_stream_api.zig`
```zig
pub fn handleStreamZombie(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, zombie_id: []const u8) void
pub fn handleZombieMessage(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, zombie_id: []const u8) void
```

### New: `redis_zombie.xaddZombieMessage`
```zig
pub fn xaddZombieMessage(client: *redis_client.Client, zombie_id: []const u8, message: []const u8) ![]const u8
```

### Modified: `event_loop.runEventLoop` / `processEvent`
Adds fire-and-forget `cfg.redis.publish()` call after each event is processed.
No signature change.

---

## Spec-Claim Tracing

| Claim | Test | Status |
|---|---|---|
| Pub/sub publish called on event processed | §1.2 unit | PENDING |
| Publish failure does not crash loop | §1.3 unit | PENDING |
| End-to-end pub/sub round-trip | §1.4 integration | PENDING |
| SSE stream emits zombie_activity events | §2.1 acceptance criterion 1 | PENDING |
| Last-Event-ID replay | §2.2 + acceptance criterion 3 | PENDING |
| Poll fallback on Redis down | §2.3 | PENDING |
| xaddZombieMessage XADD format | §3.3 unit | PENDING |
| Injected message reaches event loop | §3.4 integration | PENDING |
| 409 on inactive zombie | §3.2 + acceptance criterion 5 | PENDING |

---

## Verification Plan

```bash
# Build
make build
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux

# Lint
make lint

# Unit tests
make test

# Integration (requires make up)
make test-integration

# Manual SSE check
curl -N -H "Authorization: Bearer $TOKEN" \
  http://localhost:7000/v1/zombies/$ZOMBIE_ID/stream

# Manual chat-inject check
curl -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"message":"ping from operator"}' \
  http://localhost:7000/v1/zombies/$ZOMBIE_ID/message
```
