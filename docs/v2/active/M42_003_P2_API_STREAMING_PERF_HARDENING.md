# M42_003: Streaming Substrate — Performance Hardening (Hot-Path Cleanup)

**Prototype:** v2.0.0
**Milestone:** M42
**Workstream:** 003
**Date:** Apr 29, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — pre-alpha is acceptable as-is; this lands before any meaningful concurrent-zombie or chunk-heavy load. Filed as a follow-up from the M42_001 PR review (specialist sweep, 4/4 specialists in agreement).
**Categories:** API, INFRA
**Branch:** feat/m42-003-streaming-perf

## Goal

Eliminate four avoidable per-frame costs in the worker → SSE hot path that the M42_001 specialist review surfaced. None block the milestone shipping; all matter once we scale beyond ~10 concurrent SSE clients or chunk-heavy zombie responses.

## Scope (what "streaming substrate hardening" means)

The streaming substrate is the worker → Redis pub/sub → SSE handler pipe that surfaces live zombie activity to the dashboard:

```
worker (single thread per zombie)
   ├── activity_publisher.publish*()  → Redis PUBLISH zombie:{id}:activity   (ephemeral live tail)
   └── XADD core:events                                                       (durable backfill)
                                              ↓
                                    SSE handler subscribes,
                                    fans out to N dashboard tabs
```

This spec hardens the per-frame cost of that pipe. User-visible payoff: a chunk-heavy LLM response (e.g. 200 chunks over 4 s) does not stutter when multiple dashboard tabs are open against the same zombie. Not a correctness fix — the path is correct today — but a steady-state alloc + lock + index hygiene pass that prevents the worker thread from blocking on its own publish path.

Performance targets (validated via `make bench` pre/post):
- Per-frame allocator round-trips: ~3 → 0 in steady state (Findings 1+2).
- Worker mutex contention between PUBLISH and stream commands: eliminated (Finding 3).
- `GET /events` p95 on a high-volume zombie (10k+ rows): index seek instead of sort-and-scan (Finding 4).

## Findings (from M42_001 PR review)

### 1. Per-frame `valueAlloc` ×6 in publish loop

`src/zombie/activity_publisher.zig:51,79,100,120,140,160` — every helper (`publishEventReceived`, `publishToolCallStarted`, `publishChunk`, `publishToolCallProgress`, `publishToolCallCompleted`, `publishEventComplete`) shapes a fresh JSON payload via `std.json.Stringify.valueAlloc(alloc, .{...}, .{})`, defers the free, calls `publishRaw`. For chunk-heavy responses that's one heap alloc + free per token batch.

**Single-publisher invariant:** the file's own docstring states "Single publisher (the worker that owns the zombie's event loop)". The emitter is single-threaded per zombie — a reusable scratch buffer is safe by construction.

**Fix:** thread a per-emitter scratch `std.ArrayList(u8)` through the call sites; switch encoding to `std.json.Stringify.value(value, .{}, list.writer(alloc))`; `clearRetainingCapacity()` between frames. After warmup the buffer holds peak frame size (~1KB) and never re-allocates. Steady-state allocator round-trips → zero.

### 2. Double-parse in `transport.sendRequestStreaming`

`src/executor/transport.zig:282-298` — every progress frame is parsed once to discriminate progress vs terminal, then `pc.decodeProgress` parses the same bytes a second time to extract fields. Two arena allocations + two JSON walks per frame.

**Fix:** parse once into `std.json.Value`, branch on `is_progress`, pass the parsed value into a new `pc.decodeProgressFromValue(value)` helper. One parse per frame.

### 3. Mutex-locked shared queue client for PUBLISH

`src/queue/redis_client.zig:7,50,221` — the `Client` carries a single `std.Thread.Mutex` and every `command()` (incl. `commandUnlocked` from `commandAllowError`) acquires it. The worker's `processEvent` uses the same client for XADD, XACK, XAUTOCLAIM, AND every `activity_publisher.publishRaw` PUBLISH. Every progress frame contends with the writepath's stream commands. Contention scales with concurrent zombies × chunks per zombie.

**Fix:** give the worker a dedicated PUBLISH-only `Client` (pub/sub is best-effort and shouldn't share the queue client). At ~10 workers × 2 boxes the extra TCP connections are negligible for Redis; not worth the complexity of an async pipeline queue. The dedicated client deinits with the worker.

### 4. Index hygiene for per-zombie listing

`schema/019_zombie_events.sql` declares `zombie_events_actor_idx (zombie_id, actor, created_at DESC)` and `zombie_events_workspace_idx (workspace_id, created_at DESC)`. The most common read path in `src/state/zombie_events_store.zig:127-128` is `WHERE workspace_id=$1 AND zombie_id=$2 ORDER BY created_at DESC, event_id DESC LIMIT N` — no actor predicate. Postgres can't use the actor index efficiently; planner falls back to scan-and-sort on busy zombies.

Actor filtering uses `actor LIKE $X` (not equality) — see `src/state/zombie_events_store.zig:90,106,121,155,170,183`. The actor index only helps for left-anchored LIKE patterns; on `'%foo%'` it is unused either way. The 80/20: no-actor listing is the dashboard's primary view; actor filter is a rare sub-view.

**Fix (drop + add, single migration):**

- DROP `zombie_events_actor_idx`.
- CREATE `zombie_events_zombie_idx ON core.zombie_events (zombie_id, created_at DESC, event_id DESC)` — covers no-actor listing AND keyset-cursor `(created_at, event_id) <` comparison.

Trade-off: actor-filtered queries fall back to "seek by zombie_id on the new index, scan-and-filter on actor". With LIMIT 50 and the most-recent-rows-first ordering, this satisfies the limit in a few pages even on chatty zombies. If actor filtering becomes a measured bottleneck later, add a partial or expression index then. One index instead of two also reduces write amplification on the hot insert path.

## Acceptance

- [ ] `make bench` runs cleanly; if broken, repaired in this branch. Baseline captured pre/post.
- [ ] `make memleak` clean; final 3 lines of output pasted into PR Session Notes (worker lifecycle touched).
- [ ] `make test` clean.
- [ ] `make test-integration` clean.
- [ ] `make down && make up && make test-integration` clean (schema migration touched).
- [ ] Schema change lands as proper migration (post-v2.0.0 era; teardown rules don't apply): `DROP INDEX zombie_events_actor_idx; CREATE INDEX zombie_events_zombie_idx ...`.
- [ ] Worker publisher Redis client is separate from queue client; the `Client` mutex no longer contends across PUBLISH and stream commands.
- [ ] `activity_publisher` helpers reuse a per-emitter scratch buffer; no `valueAlloc` on the steady-state frame path.
- [ ] `transport.sendRequestStreaming` parses each progress frame exactly once.

## Out of scope

- SSE concurrency cap — separate hardening item (M42_004).
- Pattern-subscribe fan-out aggregator (`zombie:*:activity` multiplex) — future scaling work, not pre-alpha.
- Reintroducing an actor-specific index — deferred until a measured query regression justifies it.
