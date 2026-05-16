# M69_004: Connection pool + subscriber unification land the audit's P0/P1/P2 recommendations

**Prototype:** v2.0.0
**Milestone:** M69
**Workstream:** 004
**Date:** May 14, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — fixes M42_003's single-mutex contention bottleneck; throughput today is hard-capped at ~40 ops/sec/connection across the whole server.
**Categories:** API, INFRA, OBS
**Batch:** B1 — parallel-worktree compatible with M68 (zero queue-file overlap with M68's pending slices verified).
**Branch:** feat/m69-004-redis-pool
**Depends on:** M42_003 (contention diagnosis the audit references), M68_001 §O (audit deliverable at `src/queue/AUDIT.md` is the spec brief).
**Provenance:** LLM-drafted (claude-opus-4-7, 2026-05-14) from Captain Q&A scoping 2026-05-14 + the audit at `src/queue/AUDIT.md`.

**Canonical architecture:** `docs/architecture/bastion.md` (queue layer position) + `docs/architecture/data_flow.md` (XADD / pubsub paths the pool serves).

---

## Implementing agent — read these first

1. `src/queue/AUDIT.md` — the audit driving this spec. Every recommendation here is grounded with a `file.zig:LN-LN` citation. Read end-to-end. Sections "Per-dimension analysis" and "Concrete recommendations" are the implementation guide.
2. `~/Projects/oss/redis.zig/src/Pool.zig` — reference pool implementation to mirror. `SinglyLinkedList` of idle connections, `acquire`/`release` with health flag, `max_idle` cap. ~105 lines total.
3. `~/Projects/oss/redis.zig/src/Client.zig` lines 66–110 — `withConnection` retry pattern (resumable vs non-resumable error distinction via `Protocol.isResumable`).
4. `~/Projects/oss/redis.zig/src/Protocol.zig` lines 30–35 — `isResumable` predicate: server-level errors are resumable, transport errors aren't.
5. `~/Projects/oss/zig-okredis/` — alternative reference implementation. M68_001's `src/queue/AUDIT.md` compares its half-pipelining model against `redis.zig`'s pool-of-N model. M69_004 chooses pool-of-N (audit verdict: half-pipelining is worse than no pipelining for our workload). Read the comparison rows in `AUDIT.md` "Per-dimension analysis" before deviating from `redis.zig`'s shape.
6. `docs/architecture/data_flow.md` §"Two streams + one pub/sub channel" and §"Connection topology" — the architecture-level view of which Redis surfaces use the pool and which hold dedicated connections. M69_004 implements that topology; the architecture doc is authoritative for the carve-out.
7. `docs/ZIG_RULES.md` — applies to every file in this spec. Multi-step init `errdefer` chain, tagged unions for results, pg-drain lifecycle (doesn't apply here but the pattern of "drain before deinit" generalizes), cross-compile mandatory.

---

## Applicable Rules

- **`docs/ZIG_RULES.md`** — full file. Pay attention to: multi-step init `errdefer` chain pattern (the pool's `acquire` allocates a Connection outside the lock — failure path needs clean teardown); cross-compile mandatory (`zig build -Dtarget=x86_64-linux && -Daarch64-linux`); 50-line method cap.
- **`docs/greptile-learnings/RULES.md`** — RULE NLR (touch-it-fix-it on every `src/queue/` file edited), RULE NDC, RULE ORP (orphan sweep after deleting `redis_pubsub.zig`), RULE UFS (env var names follow existing `REDIS_*` prefix).
- **`docs/EXECUTE_DOC_READS.md`** — `*.zig` edits trigger ZIG_RULES.md re-read.
- ZIG GATE fires on every edit.
- PUB GATE fires on new `pub fn` surface in `redis_pool.zig`.
- LIFECYCLE GATE fires on `init`/`deinit`/`acquire`/`release` reshape.

---

## Overview

**Goal (testable):** After M69_004 lands, `src/queue/redis_client.zig` no longer holds a `std.Thread.Mutex` across the network round-trip. Concurrent XADD / PUBLISH / XACK calls from worker threads + HTTP handlers each acquire their own pooled connection (default pool size 8, configurable via `REDIS_POOL_MAX_IDLE`), execute their command without contending on a shared lock, and release. A throughput bench (`tests/bench/redis_xadd_concurrency_bench.zig`) lands in this milestone and captures absolute ops/sec for the new pool against local Redis with 8 concurrent producers — operator-facing evidence, not a ratio assertion (the single-mutex baseline was eliminated by the slice-3 Client swap before any measurement could be taken on this branch; bench is observational from this milestone forward). `src/queue/redis_pubsub.zig` is deleted; its sole consumer (test harness) retargets to a unified `Subscriber` in `redis_subscriber.zig` with an `InitOptions { read_timeout_ms: ?u32 = null }` parameter. Request-path connections honor `REDIS_REQUEST_TIMEOUT_MS` (default 5000 ms) so a frozen Upstash proxy can't pin a worker thread indefinitely. Redis-level error messages (e.g. `READONLY`, `BUSYGROUP`) are logged before being mapped to `error.RedisCommandError` so operators see the underlying cause.

**Problem:** Three problems compounded in one file:

1. **Contention.** `redis_client.zig:8` holds one `std.Thread.Mutex`; `:167-186` serializes every command's full write+read round-trip. Across all worker threads + HTTP handlers sharing the `*Client`, throughput tops out at ~40 ops/sec for a 25-ms Upstash round-trip — regardless of how many CPU cores or how many concurrent producers exist. This is M42_003's diagnosed bottleneck.
2. **Subscriber duplication.** Two near-identical `Subscriber` implementations exist (`redis_pubsub.zig` 152 lines, `redis_subscriber.zig` 147 lines) with ~85% byte-for-byte overlap; they diverge only on whether `SO_RCVTIMEO` is set. This is duplication, not dead code — RULE NLR applies on every queue edit.
3. **Operator blindness.** When a Redis command fails (e.g. failover causes `READONLY`, consumer-group race causes `BUSYGROUP`), the server-side error message is freed at `redis_client.zig:160-162` before it's logged — operators see only `ERR_INTERNAL_OPERATION_FAILED` with no underlying cause.

**Solution summary:** Adopt the reference `redis.zig`'s pool pattern: a `Pool` struct with `eager_min = 2` preconnect (env-overridable via `REDIS_POOL_EAGER_MIN`) and `max_idle = 8` cap (env-overridable via `REDIS_POOL_MAX_IDLE`). The pool serves short-lived request-path commands only (XADD, PUBLISH, XACK from HTTP handlers and worker per-step publishes); long-lived blocking consumers (watcher's XREADGROUP on `zombie:control`, per-zombie workers' XREADGROUP on `zombie:{id}:events`, SSE subscribers on `zombie:{id}:activity`) hold dedicated connections — see `docs/architecture/data_flow.md` §"Connection topology" for the load-bearing picture. Move the per-call lifecycle into `Pool.acquire`/`release` so all existing call sites stay `try client.command(&.{...})` — the lifecycle is hidden. Distinguish resumable Redis-level errors from non-resumable transport errors via an `isResumable` predicate; resumable errors recycle the connection, transport errors close + reopen. Fold the two Subscribers into one with an optional `read_timeout_ms`. Add a request-path `setsockopt(SO_RCVTIMEO)` for the new `REDIS_REQUEST_TIMEOUT_MS` knob. Log `value.err` before discarding it. P2 cleanups (compile-fold the XADD control argv; drop 16 KiB→4 KiB buffers) ride along since the files are already being touched (RULE NLR).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/queue/redis_pool.zig` | CREATE | The new pool: `SinglyLinkedList` of idle connections, `acquire`/`release` with health flag, `max_idle` cap, `eager_min` preconnect. ~120 lines target. |
| `src/queue/redis_connection.zig` | CREATE | Per-connection state extracted from today's `Client` body. Owns its `Transport` + read/write buffers. No mutex. Caller-owned during `acquire`/`release`. ~200 lines target. |
| `src/queue/redis_errors.zig` | CREATE | `isResumable(err)` predicate. ~30 lines. |
| `src/queue/redis_client.zig` | EDIT | Becomes a thin façade: owns the `Pool`, wraps `acquire → conn.command → release` with retry loop. Drops the `std.Thread.Mutex`. P2: compile-fold the XADD control argv (the 6 constant slots `"XADD"`, key, `"MAXLEN"`, `"~"`, `"10000"`, `"*"`). |
| `src/queue/redis_transport.zig` | EDIT | Add `setReadTimeout(ms: ?u32)` honoring `SO_RCVTIMEO`. P2: drop 16 KiB → 4 KiB buffer default (still env-overridable). |
| `src/queue/redis_pubsub.zig` | DELETE | Folded into `redis_subscriber.zig`. |
| `src/queue/redis_subscriber.zig` | EDIT | Accepts `InitOptions { read_timeout_ms: ?u32 = null }`. Production passes `null` (block forever). Test harness passes `25_000`. |
| `src/queue/redis.zig` | EDIT | Façade re-export updates: `Subscriber` now points at `redis_subscriber.Subscriber`. |
| `src/zombie/test_harness_helpers.zig` | EDIT | Retarget from `redis_pubsub` to `redis_subscriber.connect(..., .{ .read_timeout_ms = 25_000 })`. |
| `src/zombie/event_loop_harness_heartbeat_test.zig` | EDIT | Same retarget. |
| `src/cmd/serve.zig` | EDIT | Wire pool init at server boot. Read `REDIS_POOL_EAGER_MIN` (default 2) and `REDIS_POOL_MAX_IDLE` (default 8) from env. No derivation from worker count — per-zombie workers are spawned dynamically by the watcher post-boot. |
| `src/errors/error_registry.zig` | EDIT | Add `error.RedisRequestTimeout` mapping. |
| `src/observability/metrics_redis_pool.zig` | CREATE | Pool counters/gauges (dials_total, overflow_dials_total, poisoned_connections_total, reconnects_total, forced_closes_total, acquire_timeouts_total, acquire_wait sliding histogram). Mirrors the `executor_metrics.zig` shape. ~80 lines target. |
| `src/observability/metrics.zig` | EDIT | Re-export `inc*` / `snapshot` from `metrics_redis_pool.zig`. Mirrors existing executor-metrics re-export block. |
| `src/observability/metrics_render.zig` | EDIT | Render Pool metrics into `/metrics` Prometheus output. Mirrors executor render block. |
| `tests/integration/redis_pool_test.zig` | CREATE | Pool acquire/release, max_idle behavior, eager preconnect, retry loop. |
| `tests/integration/redis_subscriber_unified_test.zig` | CREATE | Subscriber with and without read_timeout_ms. |
| `tests/integration/redis_request_timeout_test.zig` | CREATE | SO_RCVTIMEO fires when the server doesn't respond. |
| `tests/bench/redis_xadd_concurrency_bench.zig` | CREATE | XADD concurrency bench: 8 producer threads against local Redis. Observational — captures aggregate ops/sec for the new pool. See §8 for the methodology pivot. |

---

## Sections (implementation slices)

### §1 — Pool

#### Connection topology — why the pool is for request-path commands only

usezombie has **three distinct Redis usage shapes**. A pool fits one of them; pooling the others would actively harm throughput. The spec carves them out explicitly so the implementing agent doesn't accidentally migrate the wrong consumer:

```
                        REDIS CONNECTION TOPOLOGY (M69_004)
                        ═══════════════════════════════════

  ┌─────────────────────────────────────────────────────────────────────────────────┐
  │                        POOL  (max_idle=8, eager_min=2)                           │
  │                  ──── short-lived request-path commands ────                     │
  │                                                                                  │
  │   acquire → command → release    (microseconds to milliseconds per cycle)        │
  │                                                                                  │
  └────▲─────────────────────────────▲────────────────────────────▲──────────────────┘
       │                             │                            │
       │ XADD                        │ XADD                       │ PUBLISH
       │ /messages                   │ control_stream             │ activity (per
       │ from steer/install          │ from install path          │  worker step)
       │                             │                            │
   ┌───┴────────┐               ┌────┴────────┐              ┌────┴──────────┐
   │ HTTP       │               │ Watcher     │              │ Worker after  │
   │ handlers   │               │ thread      │              │ each step     │
   └────────────┘               └─────────────┘              └───────────────┘


  ┌──────────────────────────────────────────────────────────────────────────────────┐
  │                   DEDICATED CONNECTIONS  (NOT in the pool)                        │
  │                    ──── long-lived blocking reads ────                            │
  │                                                                                   │
  │   Watcher: 1 dedicated conn       Per-zombie worker: 1 conn per zombie thread     │
  │   ─────────────────────────       ─────────────────────────────────────────       │
  │                                                                                   │
  │   ┌──────────────────┐            ┌─────────────────┐  ┌─────────────────┐        │
  │   │ XREADGROUP       │            │ XREADGROUP      │  │ XREADGROUP      │        │
  │   │ zombie:control > │            │ zombie:Z1:events│  │ zombie:Z2:events│  ...   │
  │   │ BLOCK 5000ms     │            │ BLOCK 5000ms    │  │ BLOCK 5000ms    │        │
  │   │   (loops)        │            │   (loops)       │  │   (loops)       │        │
  │   └──────────────────┘            └─────────────────┘  └─────────────────┘        │
  │                                                                                   │
  │   SSE subscribers: 1 conn per live tail                                           │
  │   ─────────────────────────────────────                                           │
  │                                                                                   │
  │   ┌──────────────────────┐       ┌──────────────────────┐                         │
  │   │ SUBSCRIBE            │       │ SUBSCRIBE            │                         │
  │   │ zombie:Z1:activity   │       │ zombie:Z2:activity   │  ...                    │
  │   │  (held while SSE     │       │  (held while SSE     │                         │
  │   │   client is open)    │       │   client is open)    │                         │
  │   └──────────────────────┘       └──────────────────────┘                         │
  │                                                                                   │
  └───────────────────────────────────────────────────────────────────────────────────┘
```

**Why the carve-out matters:** with `max_idle = 8`, if every per-zombie worker pulled a pooled connection for its `XREADGROUP BLOCK 5000` loop, the **9th zombie cannot install**. Pool exhausted, workers stuck. usezombie's design supports ~unbounded zombies per worker process; a pooled XREADGROUP defeats that. Same logic for SSE subscribers — a customer with 100 dashboard tabs open should not exhaust the pool.

#### Pool implementation

`Pool` owns `idle: std.SinglyLinkedList`, `idle_count: usize`, `max_idle: usize`, `eager_min: usize`, and a `std.Thread.Mutex` held **ONLY** across linked-list pop/push (microseconds, not network calls). `acquire` returns from idle if non-empty; on miss, creates a new `Connection` **outside** the lock to avoid blocking other pool users during dial. `release(conn, ok)` closes the connection when `ok=false` (transport error), closes when over `max_idle`, else pushes idle.

**Implementation defaults:**

- `max_idle = 8`, configurable via `REDIS_POOL_MAX_IDLE` env.
- `eager_min = 2`, configurable via `REDIS_POOL_EAGER_MIN` env. Two pre-warmed connections cover the boot window — the watcher's first `XADD zombie:control` on install + the first HTTP handler's `XADD .../messages`. **There is no boot-time worker count to derive from**: per-zombie workers are spawned dynamically by the watcher at zombie claim time (`Thread.spawn` in `src/cmd/worker_watcher.zig`). Earlier drafts of this spec said `eager_min = worker_count`; that was incoherent and has been removed.

### §2 — Connection

Extract everything that's per-connection out of today's `Client`: `Transport`, read/write buffers, `Config`, dial logic. No mutex on `Connection`. Caller-owned for the lifetime of an `acquire`. `Connection.command(argv)` is the unit of work — write argv as RESP, read response, return `RespValue`.

**Implementation default:** keep today's RESP2 parser and write-side argv encoder; the audit's Dimension 1 verdict was "leave alone unless bench shows allocator pressure." The XADD argv compile-fold (P2) is a separate slice (§7).

**`ConnectionRole` is a correctness-boundary field, not observability metadata.** It governs `Pool.acquire` return-shape, `Pool.release` accept-shape, `Subscriber.connect` accept-shape, and the dedicated XREADGROUP loop's construction site. Role is set at `Connection.init` time via a `role: Role` parameter and is `const` for the connection's lifetime — boundary code (`Pool.release`, `Subscriber.connect`) asserts the role on receipt; nothing inside `Connection` mutates `self.role` after init. Three role-named constructors were considered (`initPooled` / `initDedicated` / `initSubscriber`) but rejected: same `Connection` return type means the role check stays runtime regardless, so the wrappers added LOC without adding compile-time safety.

```zig
pub const Role = enum { pooled, blocking_consumer, subscriber };

// `ConnectionState` and `transitionTo` stay file-private — the invariants
// they enforce (10, 14) operate inside Connection's own methods, and
// boundary code reads `state` via the field with literal-typed comparisons
// (`.poisoned` / `.active`). `INVALID_FD` is pub so slice-1 tests in
// `redis_connection_test.zig` can verify Invariant 13's post-close zero.

const Connection = @This();

role: Role,              // const after init — see Invariant 7
state: ConnectionState,  // mutate ONLY via transitionTo() — see Invariant 14
fd: std.posix.fd_t,      // set to INVALID_FD after close — see Invariant 13
transport: Transport,
alloc: std.mem.Allocator,
cfg: *const Config,      // borrowed from Pool / spawning thread

pub fn init(alloc, cfg, role: Role) !Connection {}
pub fn deinit(self: *Connection) void {}

// The source-side anchor comment `// PIPELINING FORBIDDEN — see spec
// §Invariants 12.` is mandated by Invariant 12 and lives at the call site
// in `redis_connection.zig`.
pub fn command(self: *Connection, argv: []const []const u8) RedisError!RespValue {}

fn transitionTo(self: *Connection, new_state: ConnectionState) void {}
```

**Legal `ConnectionState` transitions** (anything else asserts in debug):

| From | To | Trigger |
|---|---|---|
| `.active` | `.poisoned` | Parser ambiguity, residual bytes after reply, partial frame on timeout |
| `.active` | `.closing` | Graceful release path (Pool deinit, thread shutdown) |
| `.poisoned` | `.closing` | `Pool.release(conn, ok=false)` after poison |
| `.closing` | `.closed` | `closeFd()` succeeds; `state = .closed` set as final step |

`.closed` is terminal and reachable exactly once per Connection lifetime.

### §3 — Client façade with retry

`Client.command(argv)` becomes the public surface. Internally: acquire from pool, call `Connection.command`, distinguish via `isResumable` (server-level error = recycle the connection, transport error = close + fresh), retry up to N=2 attempts default. Release with `ok` flag set accordingly. Existing call sites (`try client.command(&.{...})`) unchanged — Pool/Connection lifecycle hidden.

**Implementation default:** retry attempts = 2 (audit cite: `~/Projects/oss/redis.zig/src/Client.zig:27`). Per-operation retry contracts are codified in §3a; the Client implementation must match that table exactly.

### §3a — Retry Contracts (per-operation table)

The retry posture differs by operation and connection role. This table is the load-bearing contract — every implementation site must conform.

| Path | Operation | Idempotent on retry? | Failure → action | Max attempts | Backoff | Give-up posture |
|---|---|---|---|---|---|---|
| **Pool** | `XADD zombie:{id}:events` | NO at Redis layer · **YES at PG layer** via `INSERT ON CONFLICT (zombie_id, event_id) DO NOTHING` on `core.zombie_events` | Resumable err → same conn, retry; Transport err → close conn (state→poisoned→closing→closed), dial fresh, retry | 2 | none (immediate) | After 2: surface `error.BrokenPipe` / `error.RedisCommandError` to caller |
| **Pool** | `PUBLISH zombie:{id}:activity` | YES (pub/sub is lossy by design; dropped frames acceptable — durable record in `core.zombie_events`) | Same as XADD | 2 | none | After 2: log + drop frame (best-effort surface) |
| **Pool** | `XACK zombie:{id}:events <stream_id>` | YES (XACK on already-acked entry is a no-op in Redis consumer groups) | Same as XADD | 2 | none | After 2: log + continue; PEL reclaim recovers via `XAUTOCLAIM` eventually |
| **Dedicated** | Watcher `XREADGROUP zombie_workers ... zombie:control >` | YES (consumer-group resumes from PEL) | On any error: `Connection.deinit` (poisoned → closing → closed), sleep backoff, dial fresh via `Connection.init(alloc, cfg, .blocking_consumer)`, re-issue `XREADGROUP` | **unlimited** | exponential 100ms → 30s + 50% jitter | **Never** — watcher is durable; a 5-min Redis outage must not kill the thread |
| **Dedicated** | Per-zombie worker `XREADGROUP zombie_workers ... zombie:{id}:events >` | YES (same CG semantics) | Same as watcher | **unlimited** | exponential 100ms → 30s + 50% jitter | **Never** — zombies are durable runtime instances; thread survives outage |
| **Subscriber** | SSE `SUBSCRIBE zombie:{id}:activity` | YES (pub/sub lossy; missed frames fall back to durable `GET /events`) | On any error: `nextMessage` returns `null` → SSE handler closes the response → HTTP client reconnects → fresh `Subscriber.connect` in the new handler | **0** at Redis layer | n/a — retry is HTTP-layer responsibility | One-strike at the Redis-conn layer; **no internal Redis retry** |

**Backoff with jitter** (mandatory for dedicated-connection reconnect, **not** optional):

```zig
const base_ms: u32 = @min(100 * (@as(u32, 1) << @intCast(attempt)), 30_000);
const jitter_window_ms: u32 = base_ms / 2;
const sleep_ms = base_ms + std.crypto.random.intRangeAtMost(u32, 0, jitter_window_ms);
```

Without jitter, a Redis restart causes synchronized reconnect from 100+ dedicated threads → self-induced TLS-handshake thundering herd. With 50%-of-base jitter, reconnects spread across a 50% window per attempt level.

**Load-bearing consequences:**

1. **XADD has at-least-once delivery to the Redis stream.** Connection dies after server processes the write but before reply reaches client → client retries → duplicate stream entry. Worker XREADGROUPs both, hits `INSERT ON CONFLICT` for the second, becomes no-op. See §Delivery Semantics for the full picture.
2. **Watcher/worker XREADGROUP retry IS the thread loop, not a retry counter.** The thread's main `while (!shutdown_flag.load())` loop is the retry primitive. On transport error: `errdefer conn.deinit()`, sleep with jitter, next iteration dials fresh. **Do not introduce a separate `RetryPolicy` abstraction**; the loop has one strategy, hard-coded.
3. **SSE has zero Redis-layer retry by design.** The SSE handler treats a `null` from `nextMessage` as a clean disconnect, returns 200 to the HTTP client, browser EventSource auto-reconnects via HTTP (new request → new handler → new Subscriber → new dedicated connection). Adding Redis-layer retry to the Subscriber would create double-recovery semantics that conflict with the SSE-reconnect protocol.

### §4 — `isResumable` predicate

`src/queue/redis_errors.zig` exposes `isResumable(err: RedisError) bool` over an **exhaustive `switch`** on a typed `RedisError` set. Compiler-enforced: adding a new error variant forces an explicit resumable-vs-not decision at compile time, not "did I remember to update the predicate?" at review time.

```zig
pub const RedisError = error{
    RedisCommandError,      // server-side error (READONLY, BUSYGROUP, WRONGTYPE...) — RESUMABLE
    RedisXaddFailed,        // typed sibling — RESUMABLE
    RedisXackFailed,        // typed sibling — RESUMABLE
    BrokenPipe,             // transport — NOT resumable
    ConnectionResetByPeer,  // transport — NOT resumable
    ReadFailed,             // transport — NOT resumable
    WriteFailed,            // transport — NOT resumable
    RedisRequestTimeout,    // SO_RCVTIMEO fired — NOT resumable (connection closed)
};

pub fn isResumable(err: RedisError) bool {
    return switch (err) {
        .RedisCommandError, .RedisXaddFailed, .RedisXackFailed => true,
        .BrokenPipe, .ConnectionResetByPeer, .ReadFailed, .WriteFailed, .RedisRequestTimeout => false,
    };
}
```

Mirror `redis.zig`'s `Protocol.isResumable` shape but use Zig's switch-exhaustiveness instead of a list-of-strings. Any future error added to `RedisError` is a compile error until classified.

### §5 — Subscriber unification

Delete `src/queue/redis_pubsub.zig`. Extend `src/queue/redis_subscriber.zig` with `InitOptions { read_timeout_ms: ?u32 = null }`. When non-null, set `SO_RCVTIMEO` and treat `error.ReadFailed` as a clean null-on-timeout (so the test budget loop advances). When null, block forever and surface only `EndOfStream` / `ReadFailed` as null (production SSE behavior). Retarget the two test harness consumers. Update the façade `redis.zig:11` re-export.

### §5b — XREADGROUP consumers stay outside the pool

The blocking-XREADGROUP consumers — `src/queue/redis_zombie.zig` (per-zombie events loop) and `src/cmd/worker_watcher_poll.zig` (watcher's control-stream loop) — are **NOT** migrated to the pool. They each own a dedicated `Connection` constructed via `Connection.init(...)` for the lifetime of the watcher / per-zombie worker thread. The connection is teardown-owned by its thread; pool has no awareness of it.

Concretely, after M69_004:

- `Pool.acquire/release` serves: HTTP handler XADDs, watcher's transient XADDs (install path), worker's per-step PUBLISH on `zombie:{id}:activity`.
- Dedicated `Connection`: watcher's blocking `XREADGROUP zombie:control >`, per-zombie worker's blocking `XREADGROUP zombie:{id}:events >`.
- Dedicated `Subscriber`: each SSE handler's `SUBSCRIBE zombie:{id}:activity`.

The pool has no awareness **of dedicated connections held by other threads** — but it absolutely tracks its own active/idle/shutdown/health state (see `PoolStats` in §Interfaces). This is the load-bearing architectural decision of M69_004; the §1 topology diagram is authoritative.

### §6 — Request-path read timeout

`src/queue/redis_transport.zig` exposes `setReadTimeout(ms: ?u32)` (uses `setsockopt(SO_RCVTIMEO)` like today's pubsub code at `redis_pubsub.zig:84-96`, but applied to the request-path Transport too). Default `REDIS_REQUEST_TIMEOUT_MS = 5000`. When the timeout fires, surface `error.RedisRequestTimeout` (not resumable — connection is closed by the pool).

### §7 — Surface Redis error messages + P2 cleanups

At every `value.deinit(self.alloc)` site in `redis_client.zig` that's preceded by a logged error code, log `value.err` first so operators see the underlying server message (`READONLY` after failover, `BUSYGROUP` on consumer-group races, `WRONGTYPE`, etc.). P2 ride-along (since RULE NLR applies on every queue file touched): compile-fold the XADD control argv constants.

**The 16 KiB → 4 KiB buffer reduction is REVERTED from this spec's scope.** Activity-frame `agent_response_chunk` payloads on the subscriber path can exceed 4 KiB on model streams; reducing the buffer creates fragmented reads and parser edge cases that the per-role workload sizes don't justify uniformly. Per-role buffer sizing is a bench-driven follow-up, not a P2 cleanup. Keep at 16 KiB. See §Out of Scope.

### §8 — Bench + integration tests

`tests/bench/redis_xadd_concurrency_bench.zig` runs 8 producer threads, captures absolute XADD throughput (ops/sec) for the pool-of-8 Client against local Redis. **Observational, single-stage** — the ≥4× ratio framing in earlier drafts assumed a pre-slice-3 baseline measurement on the same branch; the actual EXECUTE-order swapped the single-mutex Client before any baseline could be captured, so this milestone records the new pool's number directly and leaves ratio analysis to operator inspection against the audit-documented bottleneck (~40 ops/sec/connection, spec L8). Both spec-rule-honoring options (rebase to insert pre-slice-3 commit, or split into a follow-up M69_005) were explicitly rejected by Captain in favor of a single-stage observational bench in this milestone.

**Methodology:**

- Local Redis on `localhost:6379` (no Upstash — round-trip latency would mask any client-side effect).
- 8 producer threads, each XADDing 1000 events to distinct `bench:{thread_id}:events` streams (no shared key contention so the measurement is client-side concurrency, not Redis-server lock).
- Pool size: default (`eager_min=2`, `max_idle=8`).
- Bench runs once at HEAD. Records:
  - Wall-clock elapsed across the 8000-event burst.
  - Aggregate ops/sec.
  - Per-thread completion ordering (sanity check — all 8 threads finish within a small spread, confirming no serialization remained).
- Number pasted into PR Session Notes with the bench commit SHA.

**CI handling:** bench is marked `// skip-in-ci` (no Redis service container in CI today). Local-only evidence — agent runs the bench on their dev machine, pastes output. If a future change adds a CI Redis container, flip the skip and let CI re-run continuously.

Integration tests cover: pool acquire/release happy path; pool exhaustion (>max_idle in flight) creates new connection on the fly; release-as-broken closes the connection; subscriber with timeout fires; request timeout surfaces.

---

## Interfaces

**`src/queue/redis_pool.zig` public surface (locked shape — agent picks exact field types):**

```zig
pub const Pool = struct {
    pub fn init(alloc, cfg: Config, options: InitOptions) !Pool;
    pub fn deinit(self: *Pool) void;
    pub fn acquire(self: *Pool) !*Connection;
    pub fn release(self: *Pool, conn: *Connection, ok: bool) void;
    pub fn stats(self: *Pool) PoolStats;  // wired into /metrics — see §Observability below
};

pub const InitOptions = struct {
    max_idle: usize = 8,
    eager_min: usize = 2,
    // No `read_timeout_ms` here. Per-command timeout is set on the Connection,
    // not pool-wide — see ConnectOptions below. Pool-wide default lives in
    // the env var REDIS_REQUEST_TIMEOUT_MS (read once at boot).
};

pub const ConnectOptions = struct {
    read_timeout_ms: ?u32 = null,  // null → fall back to REDIS_REQUEST_TIMEOUT_MS env (default 5000)
};

pub const PoolStats = struct {
    // ── Utilization (snapshot) ──────────────────────────────────────────
    active: usize,                       // in-flight (acquired, not released)
    idle: usize,                         // available in idle list
    dials_total: u64,                    // cumulative dial count since boot
    overflow_dials_total: u64,           // cumulative dials past max_idle
    acquire_wait_ns_p99: u64,            // sliding-window p99 of acquire wait

    // ── Pathology (cumulative, never reset) ─────────────────────────────
    poisoned_connections_total: u64,     // incremented in release(ok=false) path
    reconnects_total: u64,               // incremented after a successful fresh dial in retry loop
    forced_closes_total: u64,            // incremented in coordinator's SIGTERM step-8 forced-close path
    acquire_timeouts_total: u64,         // reserved for future bounded-wait acquire variant
};
```

**Observability — `/metrics` wiring.** `Pool.stats()` is consumed by the existing `/metrics` endpoint in `src/observability/`. Each field renders as a Prometheus gauge or counter per the existing pattern. Operators get utilization (`am I sized right?`) and pathology (`is the transport healthy?`) from one scrape. Pathology counters are cumulative, never reset, so a `rate()` over them surfaces failure-rate trends. Wiring lives in this slice — not deferred to M70.

**`Client` public surface (UNCHANGED at call sites):**

```
try client.command(&.{ "XADD", stream_key, "*", "data", value });
try client.xaddZombieEvent(zombie_id, envelope);
// All existing call sites identical to today.
```

**Environment variables (NEW):**

- `REDIS_POOL_MAX_IDLE` — default `8`.
- `REDIS_REQUEST_TIMEOUT_MS` — default `5000`.

**Subscriber InitOptions (locked):**

```
pub const InitOptions = struct { read_timeout_ms: ?u32 = null };
// Production SSE: .{} (block forever, null on EndOfStream).
// Test harness:    .{ .read_timeout_ms = 25_000 } (SO_RCVTIMEO, null on ReadFailed).
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Pool exhausted under burst | All `max_idle` connections in-flight | `acquire` creates a new Connection on the fly (no hard cap); over-`max_idle` connections close on release rather than going back to idle. Audit §"Pool.zig:80-105" pattern. |
| Server-side Redis error | `READONLY` (post-failover), `BUSYGROUP`, `WRONGTYPE` | Connection recycled (resumable); error logged with the underlying message; mapped to `error.RedisCommandError`. |
| Transport error (broken pipe) | Network blip, server restart | Connection closed; pool acquires fresh on retry; if retry exceeds attempts, surface as `error.BrokenPipe` to caller. |
| Request timeout fires | Frozen Upstash proxy, kernel keepalive masking dead peer | `SO_RCVTIMEO` returns `error.ReadFailed`; mapped to `error.RedisRequestTimeout`; connection closed; pool gets a fresh one. |
| Subscriber EndOfStream | Redis pub/sub disconnect | Returns `null` from `nextMessage` (today's behavior preserved). Production SSE handler treats null as clean disconnect → 200 close. |
| Subscriber SO_RCVTIMEO fires (test harness) | No messages within budget | Returns `null` from `nextMessage`; test loop advances its budget. |
| Partial init failure on `Connection.init` | TLS handshake fails after socket buffers allocated | `errdefer` chain tears down each step (audit §5 "Stability" — already clean, preserve under the refactor). |
| **SIGTERM during normal operation (async-signal-safe shutdown sequence)** | Operator kill / orchestrator restart | See the locked 9-step sequence and async-signal-safety boundary table below. |
| Env var typo (`REDIS_POOL_MAX_IDLE=abc`) | Operator misconfiguration | Parse error at boot; surface as `error.ConfigParseFailed`; server fails to start with a clear log line. |

### Shutdown sequence — locked, async-signal-safe

The SIGTERM handler does **exactly one thing**: atomic store on `shutdown_flag`. No `close()`, no `log()`, no allocator, no mutex, no `join`. The handler is async-signal-safe by construction; returns immediately.

A coordinator thread (the main thread post-init) wakes on the flag and orchestrates the staged teardown:

1. **Coordinator:** stop accepting new HTTP requests (existing `server.shutdown()` pattern in `src/cmd/serve.zig`).
2. **Coordinator:** wait for in-flight HTTP handlers to drain (existing pattern).
3. **Coordinator:** set `watcher.shutdown_flag = true`.
4. **Watcher's own thread:** at the next XREADGROUP loop iteration boundary (between iterations, **not** mid-call), notices flag → `Connection.deinit` (`.active → .closing → .closed`, fd → `INVALID_FD`) → thread exits.
5. **Coordinator:** for each per-zombie worker thread, set `worker.shutdown_flag = true`.
6. **Each worker's own thread:** same pattern as watcher.
7. **Coordinator:** `join` all worker threads with `join_deadline = BLOCK_ms + 1000` (default 6000ms — slightly longer than `BLOCK 5000`).
8. **Coordinator (exceptional path):** if a thread misses `join_deadline` because its `XREADGROUP` is mid-BLOCK with no message arrival, the coordinator forcibly closes that thread's connection fd. Ownership transfer is mediated by a per-Connection `shutdown_in_progress: std.atomic.Value(bool)` set by the coordinator before `close()`; the owning thread checks this flag in its post-XREADGROUP error path and skips its own `deinit` to avoid double-close. `Pool.stats().forced_closes_total++`.
9. **Coordinator:** `Pool.deinit` walks the now-quiescent idle list, closes each connection.

**Async-signal-safety boundary:**

| Action | Signal handler | Coordinator thread | Owning thread |
|---|---|---|---|
| atomic flag store | ✓ | ✓ | ✓ |
| `close(fd)` | ✗ | ✓ (post-deadline forced-close only) | ✓ (own conn, normal path) |
| `log(...)` | ✗ | ✓ | ✓ |
| allocator | ✗ | ✓ | ✓ |
| `join` | ✗ | ✓ | ✗ |

Dedicated threads own interruption of their own sockets as the default path. The coordinator-forced close at step 8 is the **exceptional** branch and goes through explicit ownership-transfer (`shutdown_in_progress` atomic) to avoid `close()` race on a recycled kernel fd.

## Delivery Semantics — DO NOT REMOVE THE PG `ON CONFLICT` CLAUSE

The Redis stream `zombie:{id}:events` MAY contain duplicate entries for the same logical `event_id`. This is **expected**, **non-corrupting**, and **load-bearing**. It is the consequence of at-least-once XADD delivery semantics under reconnect ambiguity:

1. Client sends XADD.
2. Server processes the write successfully and assigns a stream ID.
3. Transport dies before the reply reaches the client (broken pipe, request timeout, kernel keepalive masking dead peer).
4. Client surfaces `error.BrokenPipe`, retries per §3a retry contract.
5. Second XADD lands. Redis assigns it a distinct stream ID. **Both entries are durable in the stream.**

Side-effect deduplication lives at the PostgreSQL layer:

```sql
INSERT INTO core.zombie_events (zombie_id, event_id, ...)
VALUES (...)
ON CONFLICT (zombie_id, event_id) DO NOTHING
```

**If a future engineer removes this `ON CONFLICT` clause** because *"`event_id` is unique, duplicates should never happen"* — every Redis transport error becomes a duplicate-execution bug. The Redis layer cannot be exactly-once without client-deterministic stream IDs, which Redis does not natively support and which would require distributed coordination we do not have and do not want.

The anchor comment at the INSERT site is contractual:

```zig
// DO NOT REMOVE THIS ON CONFLICT — see spec §Delivery Semantics.
// Redis stream allows duplicates by design; PG is the dedup boundary.
try conn.exec(
    "INSERT INTO core.zombie_events (zombie_id, event_id, ...) " ++
    "VALUES ($1, $2, ...) ON CONFLICT (zombie_id, event_id) DO NOTHING",
    .{ zombie_id, event_id, ... },
);
```

**Worker behavior on duplicate:**

1. XREADGROUP delivers both entries (one normal, one duplicate, distinct stream IDs).
2. `processEvent` INSERTs first; second hits `ON CONFLICT`, becomes no-op.
3. Both entries get XACKed (XACK is idempotent in consumer groups — acking an already-acked entry is a server-side no-op).
4. No duplicate side effects. Worker pays cost of one redundant PG lookup per duplicate.

**Other delivery surfaces:**

- **Redis `zombie:{id}:activity` pub/sub:** lossy by design (no persistence, no ACK, no resume). Missed frames fall back to the durable `GET /events` history. Documented in [`docs/architecture/data_flow.md`](../../architecture/data_flow.md) §"Two streams + one pub/sub channel".
- **XACK:** idempotent (Redis consumer-group semantics).
- **Durable replay:** lives in `core.zombie_events`, not in Redis. Redis streams are a transport, not a system of record.

---

## Invariants

1. `Client` holds no mutex around network I/O. `Pool` may hold a `std.Thread.Mutex` but only across linked-list pop/push (microseconds), never across `Connection.command`. Enforced by: (a) `grep -n "std.Thread.Mutex" src/queue/redis_client.zig` returns empty (the Client surface holds none); (b) code review of `src/queue/redis_pool.zig` confirms every `mutex.lock()` is paired with `mutex.unlock()` inside the same function body with no `Connection.command` / `Connection.dial` / `try` call between them.
2. Every `Pool.acquire` returns a `Connection` whose `Transport` is in a known state (connected; buffers reset; `state == .active`) — enforced by `Connection.reset()` called inside `acquire` before return.
3. `Pool.release(conn, false)` always closes the connection — enforced by code review + test `test_release_broken_closes_conn`.
4. Every `setsockopt(SO_RCVTIMEO)` call has a matching `getsockopt` verification under debug builds — enforced by the existing audit pattern at `redis_pubsub.zig:79-96` (preserved in the unified subscriber).
5. `redis_pubsub.zig` no longer exists — enforced by `test ! -f src/queue/redis_pubsub.zig`.
6. The facade `redis.Subscriber` resolves to `redis_subscriber.Subscriber` — enforced by `grep -n "Subscriber" src/queue/redis.zig`.

### Ownership and lifecycle invariants (per round-2 + round-3 engg review)

7. **A `Connection` has exactly one owner at a time.** Owner is either (a) the `Pool`'s idle list, (b) the caller of `Pool.acquire` until matched `Pool.release`, or (c) the thread that constructed it via `Connection.init(alloc, cfg, .blocking_consumer)` or `.subscriber`. Cross-owner access is undefined behavior. Enforced by: `conn.role` is set once at `init(role)` and `const` thereafter — no method on Connection mutates it; `Pool.release` asserts `role == .pooled`; `Subscriber.connect` asserts `role == .subscriber`; test `test_release_into_pool_rejects_dedicated`.

8. **Pool-owned connections may never enter SUBSCRIBE mode.** SUBSCRIBE mutates the Redis connection-state machine — most commands return `ERR Can't execute 'X': only (P)SUBSCRIBE...` after SUBSCRIBE. A previously-SUBSCRIBEd connection cannot serve request-path commands. Enforced by: `Pool.acquire` returns `role == .pooled` connections only; `Subscriber.connect` accepts `role == .subscriber` connections only; both checks happen at the type-system / constructor boundary, not at command-dispatch time.

9. **Dedicated connections may never be released into Pool.** `Pool.release` asserts `conn.role == .pooled` before adding to idle list. A thread that misroutes a dedicated connection into the pool is a programmer error caught at runtime in debug, hard-failing the boot.

10. **Pool connections must consume exactly one RESP reply per `acquire`, no more, no less, before release.** ANY of the following = `release(conn, ok=false)`, socket closed, dial fresh next time: parser error, residual bytes in read buffer after reply boundary, pending error frame, ambiguous reply shape, EOF without complete reply. There is no "best-effort drain," no "parser recovery," no "probably reusable." Cost of false-positive close = one TLS dial. Cost of false-negative reuse = silent corruption that surfaces three deploys later as a ghost bug. We pay the dial. Enforced by: `Connection.command` reads exactly one reply via a length-bounded parser; if boundary doesn't match a zero-byte read-buffer remainder, transitions `state → .poisoned`. `Pool.release` reads `state` and closes on poisoned. Tests `test_residual_bytes_after_reply_poisons_conn` and `test_partial_frame_timeout_closes_conn`.

11. **Any timeout, transport error, or unexpected EOF during read = connection closed, never reused.** Parser state is not preserved across errors; there is no "resume from partial frame" path. Enforced by: `isResumable(err)` returns false for all read-side / transport errors; `Connection.command` transitions `state → .poisoned` on any read error; integration tests `test_redis_restart_during_xreadgroup_reconnects` and `test_partial_frame_timeout_closes_conn`.

12. **Pooled connections operate strictly in synchronous request-response mode. Redis command pipelining is forbidden. Receiving bytes beyond the expected single-response boundary poisons the connection.** Anchor comment `// PIPELINING FORBIDDEN — see spec §Invariants 12.` at `Connection.command` call site marks the contract for greppability. Future engineers proposing pipelining must first remove that anchor and explain why.

13. **A Connection's `fd` is set to `INVALID_FD` (= -1) immediately after `close(fd)` succeeds.** Every IO path asserts `fd != INVALID_FD` before any read/write/setsockopt. Reusing a stale fd integer after kernel recycling is a known systems-programming hazard; the assertion catches it in test/debug before it reaches production. Release-build cost: zero (`std.debug.assert` stripped). Test `test_invalid_fd_after_close`.

14. **A Connection transitions to `.closed` exactly once.** `transitionTo(.closed)` asserts prior state is `.closing`. Catches double-deinit, retry-after-close, shutdown races, and release-after-poison through assertions rather than production incidents. The `state` field is mutated only via `transitionTo(new_state)`; direct writes are forbidden (debug assertion in helper + grep-able convention). Test `test_state_double_close_asserts`.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_pool_acquire_release_happy` | `acquire` returns a connected `Connection`; `release(conn, true)` puts it back in idle (idle_count grows). |
| `test_pool_eager_preconnect` | `Pool.init({ .eager_min = 4 })` pre-creates 4 idle connections before the first `acquire`. |
| `test_pool_max_idle_cap` | After 16 release-with-ok=true cycles and `max_idle=8`, idle_count is capped at 8; extra connections close. |
| `test_pool_release_broken_closes` | `release(conn, false)` closes the connection (not added back to idle). |
| `test_client_command_unchanged_at_call_site` | Existing call sites compile + behave identically to pre-change (regression). |
| `test_retry_resumable_recycles_conn` | When `Connection.command` returns `error.RedisCommandError`, the connection is released `ok=true` and the next retry uses the same connection. |
| `test_retry_transport_close_fresh` | When `error.BrokenPipe`, the connection is released `ok=false` (closed) and retry acquires a new one. |
| `test_xadd_pooled_throughput_observation` | Bench: 8 producer threads XADDing 1000 events each at pool-of-8 against local Redis. Records aggregate ops/sec; no ratio assertion (observational — see §8 for the methodology pivot). |
| `test_subscriber_unified_blocking` | `Subscriber.connect(.{})` blocks `nextMessage` until a message arrives or `EndOfStream`. |
| `test_subscriber_unified_timeout` | `Subscriber.connect(.{ .read_timeout_ms = 100 })` returns `null` from `nextMessage` after ~100ms when no messages arrive. |
| `test_redis_pubsub_zig_deleted` | `test ! -f src/queue/redis_pubsub.zig`. |
| `test_facade_subscriber_unified` | `redis.Subscriber == redis_subscriber.Subscriber`. |
| `test_request_timeout_fires` | Configure `REDIS_REQUEST_TIMEOUT_MS=100`; send command to a server that hangs; assert `error.RedisRequestTimeout`. |
| `test_redis_err_logged_before_deinit` | Submit a command that triggers `WRONGTYPE`; assert the log line contains the server message, not just the error_code. |
| `test_partial_init_clean_teardown` | Force TLS handshake failure mid-init; assert no leaked allocations (memleak test). |
| `test_env_var_parse_error_surface` | `REDIS_POOL_MAX_IDLE=abc` at boot → server fails to start with clear log; no zombie crash mid-request. |
| `test_cross_compile_x86_64_linux` | `zig build -Dtarget=x86_64-linux` clean. |
| `test_cross_compile_aarch64_linux` | `zig build -Dtarget=aarch64-linux` clean. |
| `test_xreadgroup_consumer_uses_dedicated_connection_not_pool` | Spin up 16 mock per-zombie workers each holding a blocking XREADGROUP loop. Assert that `Pool` with `max_idle=8` still services concurrent HTTP-handler XADDs without exhaustion. Proves the §1/§5b carve-out at runtime — workers are NOT acquiring from the pool. |
| `test_sigterm_during_pool_acquire_clean_teardown` | Fire SIGTERM while one thread is mid-`acquire` (no idle conn available, dial in flight). Assert: SIGTERM handler fires, in-flight `Connection` is torn down cleanly via the §"Failure Modes" shutdown sequence, no leaked allocations (memleak audit passes). |
| `test_residual_bytes_after_reply_poisons_conn` | Inject a `Connection.command` invocation where the read buffer contains bytes after the parsed RESP reply (simulated dirty server / mock returning extra). Assert `state == .poisoned` on return; `Pool.release` closes the connection (`state → .closing → .closed`); `stats().poisoned_connections_total` incremented. |
| `test_invalid_fd_after_close` | After `Connection.deinit`, assert `conn.fd == INVALID_FD`; subsequent read/write call paths assert-fail in debug build (test verifies the assertion fires). |
| `test_state_double_close_asserts` | Call `Connection.deinit` twice on the same connection. Assert the second call fails the `transitionTo(.closed)` precondition assertion (debug build); single-transition invariant enforced. |
| `test_redis_restart_during_xreadgroup_reconnects` | Stop Redis mid-`XREADGROUP BLOCK`; assert per-zombie worker thread reconnects via `Connection.initDedicated`, re-issues XREADGROUP, resumes from PEL within 30s (per §3a backoff + jitter cap). No leaked allocations. `stats().reconnects_total` incremented. |
| `test_partial_frame_timeout_closes_conn` | Inject `SO_RCVTIMEO` mid-multi-bulk RESP read; assert connection transitions to `.poisoned`, then `.closing`, then `.closed`; parser state not retained; next acquire dials fresh. |
| `test_subscriber_disconnect_storm_no_leak` | Open 100 SSE subscribers, kill Redis, restart; assert all 100 connections close cleanly via the SSE handler's null-on-error path, no leaked fd / allocator on subscriber teardown. |
| `test_pool_exhaustion_burst_no_starvation` | 32 concurrent threads each acquire/command/release in a tight loop for 5s; assert no thread waits > 100ms p99 for `acquire`; `stats().overflow_dials_total` increments correctly; pool returns to steady-state `idle <= max_idle` after burst ends. |
| `test_failover_reconnect_flood_completes_under_window` | Simulate Upstash primary swap (`READONLY` response from current primary, then `BrokenPipe`); assert all dedicated connections in a 100-zombie mock fleet redial within 5s with jitter spreading the dial spike; no thread hangs; `stats().reconnects_total` correctly reflects the storm. |
| `test_pool_stats_metrics_endpoint_renders` | Boot server, perform N acquire/release cycles + force M poison events, scrape `/metrics`, assert all 9 `PoolStats` fields render with correct cumulative values. Proves the observability wire-up. |

---

## Acceptance Criteria

- [ ] Every Test Specification row passes — verify: `make test && make test-integration && make memleak`.
- [ ] Bench run clean — verify: `tests/bench/redis_xadd_concurrency_bench.zig` exits 0 against local Redis; aggregate ops/sec pasted in PR Session Notes.
- [ ] No `std.Thread.Mutex` in `redis_client.zig` — verify: `grep -c "std.Thread.Mutex" src/queue/redis_client.zig` returns `0`.
- [x] `redis_pubsub.zig` deleted — verify: `test ! -f src/queue/redis_pubsub.zig`.
- [x] All `redis_pubsub` references gone — verify: `grep -rn "redis_pubsub" src/` returns 0 hits.
- [ ] `make lint` clean (includes `make check-pg-drain` and length-gate sweep).
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`.
- [ ] `gitleaks detect` clean.
- [ ] No file over 350 lines added (pool, connection, errors all target well under). Heads-up: `src/queue/redis_connection.zig` is the largest target at ~200 lines — if it crosses 250 during implementation, consider splitting RESP encode/decode into a sibling `redis_resp.zig` before crossing 350.
- [ ] Memleak clean: `make memleak`.

---

## Eval Commands

```bash
# E1: full test gauntlet
make test 2>&1 | tail -5
make test-integration 2>&1 | tail -5
make memleak 2>&1 | tail -5

# E2: XADD concurrency bench (observational; local Redis required)
zig build bench-redis 2>&1 | tail -10
# expect: exit 0 + aggregate ops/sec line; paste output into PR Session Notes

# E3: lint + pg-drain + length-gate
make lint 2>&1 | tail -5

# E4: cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3

# E5: orphan sweep — pubsub gone
test ! -f src/queue/redis_pubsub.zig && echo "PASS" || echo "FAIL"
! grep -rn "redis_pubsub" src/ && echo "PASS: no refs" || echo "FAIL"

# E6: no mutex in Client
test "$(grep -c 'std.Thread.Mutex' src/queue/redis_client.zig)" -eq 0 && echo "PASS" || echo "FAIL"

# E7: gitleaks
gitleaks detect 2>&1 | tail -3

# E8: 350L gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 }'
```

---

## Dead Code Sweep

**Orphaned files:**

| File to delete | Verify deleted |
|----------------|----------------|
| `src/queue/redis_pubsub.zig` | `test ! -f src/queue/redis_pubsub.zig` |

**Orphaned references:**

| Deleted symbol | Grep | Expected |
|---|---|---|
| `redis_pubsub` import path | `grep -rn "redis_pubsub" src/` | 0 matches |
| `PubSubMessage` (the duplicate struct) | `grep -rn "PubSubMessage" src/` | 0 matches (folded into the unified `Message`) |

---

## Skill-Driven Review Chain

| When | Skill | Required output |
|------|-------|-----------------|
| Pre-CHORE(close) | `/write-unit-test` | Coverage clean against the 18-row Test Specification + bench output. |
| Pre-CHORE(close) | `/review` | Adversarial review against `src/queue/AUDIT.md`, `docs/ZIG_RULES.md`, Failure Modes (every row gets a negative test). |
| Post `gh pr create` | `/review-pr` | Greptile pass; ZIG GATE + PUB GATE + LIFECYCLE GATE verdicts captured. |

---

## Verification Evidence

(Filled during VERIFY.)

---

## Out of Scope

- **Package extraction** (`usezombie-queue-zig` standalone repo). Audit explicit: "do the pool work IN-TREE first ... extract once the surface has stabilized." Future spec.
- **Switch to RESP3.** Out of scope; the audit doesn't recommend it.
- **PostgreSQL LISTEN/NOTIFY as a Redis replacement.** Out of scope.
- **Buffer reduction 16KiB → 4KiB.** Reverted from this spec — activity-frame `agent_response_chunk` payloads can exceed 4KiB, and per-role buffer sizing is bench-driven follow-up, not a P2 ride-along. Future spec, only if measurement shows fragmentation is a real cost.
- **Full Prometheus histogram metrics export.** `PoolStats` exposes counters/gauges into `/metrics` in this slice; full histogram-shape metrics (acquire-wait distribution, reconnect-latency distribution) are a separate M70 observability workstream.

### Rejected design directions (NOT future-work candidates)

Each item below is **seductive and would massively expand correctness surface area**. The current design wins because it is operationally understandable. That property is protected aggressively. A future spec that proposes any of these must first prove the operational model has hit a real ceiling, not a hypothetical one — and must walk through `docs/architecture/scaling.md` §"Growth paths that respect Upstash's shape" to demonstrate why the documented escape hatches are insufficient.

1. **Async runtime.** Synchronous request-response keeps parser, ownership, and shutdown semantics tractable. An async-everything redesign defeats every invariant in §Invariants 7–14.
2. **Pipelining / multiplexing commands on a pooled connection.** Forbidden by Invariant 12. Receiving bytes beyond the single-reply boundary is the corruption surface; pipelining requires async machinery that ties response to request via tags or queues, which we explicitly reject.
3. **Coalesced XREADGROUP across multiple zombies on a single dedicated connection.** Genuinely interesting (`docs/architecture/scaling.md` calls it out as a future escape valve). Out of scope here because per-zombie connection ownership cleanly supports cancellation and ownership-transfer on `drain_request`; coalescing requires a fan-out reader that re-introduces shared mutable state.
4. **Smart reconnect orchestration** (centralized reconnect handler, exponential-backoff state machine outside the consumer thread). The dedicated-thread loop IS the retry primitive. A central orchestrator creates a second authority over connection lifetime — invariant-violating by construction.
5. **Exactly-once Redis stream semantics.** Distributed-systems trap with zero operational gain. We have at-least-once transport + idempotent persistence + eventually-consistent replay. That's correct. Do not chase exactly-once via client-deterministic stream IDs.
6. **Adaptive pool sizing.** Fixed `max_idle` cap is sufficient. Revisit only if bench post-landing shows pool-of-8 still leaves contention AND raising `REDIS_POOL_MAX_IDLE` doesn't resolve it.
7. **Automatic parser recovery from partial-frame state.** Forbidden by Invariants 10–11. Any ambiguity = poison the connection. Reuse-after-partial-read is the ghost-corruption surface; "be smart" is the wrong move.
