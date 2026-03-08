# M3_004: Redis Streams — Worker Queue + Coordination

Date: Mar 4, 2026
Status: DONE
Priority: P0 — v1 requirement
Depends on: M3_001 (bug fixes must land first for CAS transitions)

---

## Problem

The current worker polls Postgres with `FOR UPDATE SKIP LOCKED` on a 2-second sleep loop. This is:

1. Wasteful — constant DB queries even when no work is queued.
2. Not horizontally scalable — no consumer-group semantics for multi-worker coordination.
3. No recovery — if a worker dies mid-run, the locked row stays locked until the connection drops.
4. No retry coordination — retries are handled by in-process re-enqueue, not durable queue semantics.

## Solution

Replace Postgres polling with Redis streams using consumer groups. Postgres remains the source of truth for run state; Redis is the queue only.

### Redis Dependency

| Environment | Provider | Notes |
|---|---|---|
| Local | Redis 7 in docker-compose | `redis:7-alpine` container |
| Development | Upstash Redis | TLS required, `usezombie-cache-dev` |
| Production | Upstash Redis | TLS required, `usezombie-cache` |

### Stream Contract

**Stream name:** `run_queue`
**Consumer group:** `workers`
**Consumer ID:** `worker-{hostname}-{pid}` (unique per process)

### API Path (enqueue)

When `handleStartRun` creates a run:

```
INSERT INTO runs (...) VALUES (...) -- Postgres, source of truth
XADD run_queue * run_id <run_id> attempt 0 workspace_id <ws_id>
```

When retry is needed (from worker):

```
XADD run_queue * run_id <run_id> attempt <n+1> workspace_id <ws_id>
XACK run_queue workers <original_message_id>
```

### Worker Path (dequeue)

```
XREADGROUP GROUP workers worker-{id} BLOCK 5000 COUNT 1 STREAMS run_queue >
```

- `BLOCK 5000` — 5-second blocking read, no busy polling.
- `COUNT 1` — one message at a time per worker.
- On successful processing: `XACK run_queue workers <message_id>`.
- On failure: message stays in PEL (pending entries list) for reclaim.

### Recovery Path (stale message reclaim)

A periodic task (every 60 seconds) on each worker:

```
XAUTOCLAIM run_queue workers worker-{id} 300000 0-0 COUNT 10
```

- Claims messages idle for >5 minutes (300000ms).
- Reclaimed messages go through normal `processNextRun` flow.
- The CAS guard in Postgres (`WHERE state = expected_state`) prevents double-processing.

### Group Initialization

On startup, the API or worker creates the group if it doesn't exist:

```
XGROUP CREATE run_queue workers 0 MKSTREAM
```

`MKSTREAM` creates the stream if it doesn't exist. Idempotent — returns OK if group already exists, or error (ignored).

## Implementation

### New Files

```
src/queue/redis.zig    — Redis client: connect, XADD, XREADGROUP, XACK, XAUTOCLAIM, XGROUP CREATE
```

### Modified Files

```
src/main.zig           — Initialize Redis connection, pass to API + worker
src/http/handler.zig   — handleStartRun: XADD after Postgres INSERT
src/pipeline/worker.zig — Replace Postgres polling with XREADGROUP loop
docker-compose.yml     — Add redis:7-alpine service
.env.example           — Add REDIS_URL
```

### Redis Client Implementation

Zig has no Redis client library in the ecosystem. Options (ordered by preference):

1. **RESP protocol directly over TCP.** Redis protocol is trivial — `*3\r\n$4\r\nXADD\r\n...`. Use `std.net.Stream` for TCP. TLS via `std.crypto.tls.Client` for Upstash.
2. **hiredis C library via Zig's C interop.** Link `libhiredis`, call via `@cImport`. More battle-tested but adds a C dependency.
3. **Shell out to `redis-cli`.** Last resort. Acceptable for local dev but not for production (subprocess overhead per command).

**Recommended:** Option 1 (native RESP). The commands needed are few (XADD, XREADGROUP, XACK, XAUTOCLAIM, XGROUP). A minimal RESP encoder/decoder is ~200 lines of Zig.

### Environment Variables

```dotenv
REDIS_URL=redis://localhost:6379
REDIS_URL_API=redis://localhost:6379       # Optional: separate for API
REDIS_URL_WORKER=redis://localhost:6379    # Optional: separate for worker
```

For Upstash (dev/prod): `rediss://default:<password>@<host>:6379` (note `rediss://` for TLS).

### docker-compose.yml Addition

```yaml
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - redis-data:/data
```

## Acceptance Criteria

1. `handleStartRun` enqueues `run_id` in Redis stream after Postgres INSERT.
2. Worker blocks on `XREADGROUP` — no Postgres polling, no sleep loop.
3. Worker ACKs message after successful run completion.
4. Failed/crashed worker's messages are reclaimed by another worker after 5 minutes.
5. `zombied doctor` checks Redis connectivity.
6. CAS guard in Postgres prevents double-processing of reclaimed messages.
7. `docker-compose.yml` includes Redis.
8. Works with Upstash Redis (TLS) in dev/prod.

## Out of Scope

- Redis pub/sub for real-time status updates (future: SSE/WebSocket to CLI).
- Redis as session store for Clerk tokens.
- Redis Cluster or Sentinel HA.

## Progress Snapshot

1. Native Redis RESP client introduced at `src/queue/redis.zig` with:
   - `XGROUP CREATE` (consumer-group bootstrap)
   - `XADD` enqueue
   - `XREADGROUP` dequeue
   - `XACK` ack
   - `XAUTOCLAIM` stale reclaim path
2. API enqueue wired in `handleStartRun` and manual retry enqueue path.
3. Worker loop now consumes queue messages first and acknowledges on successful claim execution.
4. `zombied doctor` now validates Redis API/worker connectivity (`PING`).
5. Local `docker-compose.yml` now includes `redis:7-alpine`.
6. Native TLS (`rediss://`) transport added in `src/queue/redis.zig` with CA verification:
   - system CA bundle by default (Upstash path)
   - optional `REDIS_TLS_CA_CERT_FILE` for local self-signed certs
7. D18 readiness depth hardening completed:
   - `/readyz` now includes Redis queue dependency checks (`PING` + consumer-group operability guard)
   - readiness fails closed when queue coordination is degraded, even if Postgres is healthy
   - readiness decision tests cover degraded dependency and restart windows

---

## Deferred From M4_004

1. D18 Readiness depth hardening — DONE in this track.
