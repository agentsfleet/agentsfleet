# M42_003: Streaming Substrate — Performance Hardening (Hot-Path Cleanup)

**Prototype:** v2.0.0
**Milestone:** M42
**Workstream:** 003
**Date:** Apr 28, 2026
**Status:** PENDING
**Priority:** P2 — pre-alpha is acceptable as-is; this lands before any meaningful concurrent-zombie or chunk-heavy load. Filed as a follow-up from the M42_001 PR review (specialist sweep, 4/4 specialists in agreement).
**Categories:** API, INFRA
**Branch:** _to be created_

## Goal

Eliminate four avoidable per-frame costs in the worker → SSE hot path that the M42_001 specialist review surfaced. None block the milestone shipping; all matter once we scale beyond ~10 concurrent SSE clients or chunk-heavy zombie responses.

## Findings (from M42_001 PR review)

### 1. Per-frame `valueAlloc` ×6 in publish loop

`src/zombie/activity_publisher.zig:51,79,100,120,140,160` — every helper (`publishEventReceived`, `publishToolCallStarted`, `publishChunk`, `publishToolCallProgress`, `publishToolCallCompleted`, `publishEventComplete`) shapes a fresh JSON payload via `std.json.Stringify.valueAlloc(alloc, .{...}, .{})`, defers the free, calls `publishRaw`. For chunk-heavy responses that's one heap alloc + free per token batch.

**Fix:** thread a per-emitter scratch `std.ArrayList(u8)` (or per-event `ArenaAllocator` reset between frames) through `EmitterCtx`; switch to `std.json.Stringify.value(value, .{}, list.writer(alloc))`. Steady-state allocator round-trips → zero.

### 2. Double-parse in `transport.sendRequestStreaming`

`src/executor/transport.zig:282-298` — every progress frame is parsed once to discriminate progress vs terminal, then `pc.decodeProgress` parses the same bytes a second time to extract fields. Two arena allocations + two JSON walks per frame.

**Fix:** parse once into `std.json.Value`, branch on `is_progress`, pass the parsed value into a new `pc.decodeProgressFromValue(value)` helper. One parse per frame.

### 3. Mutex-locked shared queue client for PUBLISH

`src/queue/redis_client.zig:7,50,221` — the `Client` carries a single `std.Thread.Mutex` and every `command()` (incl. `commandUnlocked` from `commandAllowError`) acquires it. The worker's `processEvent` uses the same client for XADD, XACK, XAUTOCLAIM, AND every `activity_publisher.publishRaw` PUBLISH. Every progress frame contends with the writepath's stream commands. Contention scales with concurrent zombies × chunks per zombie.

**Fix:** give the worker a dedicated PUBLISH-only `Client` (pub/sub is best-effort and shouldn't share the queue client). Or pipeline PUBLISH writes asynchronously off the writepath thread.

### 4. No covering index for per-zombie no-actor listing

`schema/019_zombie_events.sql` declares `zombie_events_actor_idx (zombie_id, actor, created_at DESC)` and `zombie_events_workspace_idx (workspace_id, created_at DESC)`. The most common read path in `src/state/zombie_events_store.zig:127-128` is `WHERE workspace_id=$1 AND zombie_id=$2 ORDER BY created_at DESC, event_id DESC LIMIT N` — no actor predicate. Postgres can't use the actor index efficiently; planner falls back to scan-and-sort on busy zombies.

**Fix:** add a new migration `CREATE INDEX zombie_events_zombie_idx ON core.zombie_events (zombie_id, created_at DESC, event_id DESC);` — covers no-actor listing AND keyset-cursor `(created_at, event_id) <` comparison.

## Acceptance

- [ ] `make bench` baseline captured pre/post against `processEvent` micro-bench.
- [ ] `make memleak` clean.
- [ ] `make test-integration` clean.
- [ ] Index 4 lands as proper `ALTER`/`CREATE INDEX` migration (post-v2.0.0 era; teardown rules don't apply).
- [ ] Worker publisher Redis client is separate from queue client; the `Client` mutex no longer contends across PUBLISH and stream commands.

## Out of scope

- SSE concurrency cap — separate hardening item (M42_004).
- Pattern-subscribe fan-out aggregator (`zombie:*:activity` multiplex) — future scaling work, not pre-alpha.
