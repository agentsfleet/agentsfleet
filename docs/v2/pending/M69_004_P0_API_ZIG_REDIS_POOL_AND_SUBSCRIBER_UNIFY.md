# M69_004: Connection pool + subscriber unification land the audit's P0/P1/P2 recommendations

**Prototype:** v2.0.0
**Milestone:** M69
**Workstream:** 004
**Date:** May 14, 2026
**Status:** PENDING
**Priority:** P0 — fixes M42_003's single-mutex contention bottleneck; throughput today is hard-capped at ~40 ops/sec/connection across the whole server.
**Categories:** API, INFRA, OBS
**Batch:** B1 — parallel-worktree compatible with M68 (zero queue-file overlap with M68's pending slices verified).
**Branch:** {feat/m69-redis-pool — added when work begins}
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

**Goal (testable):** After M69_004 lands, `src/queue/redis_client.zig` no longer holds a `std.Thread.Mutex` across the network round-trip. Concurrent XADD / PUBLISH / XACK calls from worker threads + HTTP handlers each acquire their own pooled connection (default pool size 8, configurable via `REDIS_POOL_MAX_IDLE`), execute their command without contending on a shared lock, and release. A bench test shows ≥4× throughput improvement on the XADD hot path against a local Redis with 8 concurrent producers vs the pre-change baseline. `src/queue/redis_pubsub.zig` is deleted; its sole consumer (test harness) retargets to a unified `Subscriber` in `redis_subscriber.zig` with an `InitOptions { read_timeout_ms: ?u32 = null }` parameter. Request-path connections honor `REDIS_REQUEST_TIMEOUT_MS` (default 5000 ms) so a frozen Upstash proxy can't pin a worker thread indefinitely. Redis-level error messages (e.g. `READONLY`, `BUSYGROUP`) are logged before being mapped to `error.RedisCommandError` so operators see the underlying cause.

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
| `tests/integration/redis_pool_test.zig` | CREATE | Pool acquire/release, max_idle behavior, eager preconnect, retry loop. |
| `tests/integration/redis_subscriber_unified_test.zig` | CREATE | Subscriber with and without read_timeout_ms. |
| `tests/integration/redis_request_timeout_test.zig` | CREATE | SO_RCVTIMEO fires when the server doesn't respond. |
| `tests/bench/redis_xadd_concurrency_bench.zig` | CREATE | Throughput baseline: 8 concurrent producers vs serialized — asserts ≥4× improvement post-pool. |

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

### §3 — Client façade with retry

`Client.command(argv)` becomes the public surface. Internally: acquire from pool, call `Connection.command`, distinguish via `isResumable` (server-level error = recycle the connection, transport error = close + fresh), retry up to N=2 attempts default. Release with `ok` flag set accordingly. Existing call sites (`try client.command(&.{...})`) unchanged — Pool/Connection lifecycle hidden.

**Implementation default:** retry attempts = 2 (audit cite: `~/Projects/oss/redis.zig/src/Client.zig:27`). Tunable per command if needed later, but not in v1.

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

The pool has no awareness of dedicated connections. This is the load-bearing architectural decision of M69_004; the §1 topology diagram is authoritative.

### §6 — Request-path read timeout

`src/queue/redis_transport.zig` exposes `setReadTimeout(ms: ?u32)` (uses `setsockopt(SO_RCVTIMEO)` like today's pubsub code at `redis_pubsub.zig:84-96`, but applied to the request-path Transport too). Default `REDIS_REQUEST_TIMEOUT_MS = 5000`. When the timeout fires, surface `error.RedisRequestTimeout` (not resumable — connection is closed by the pool).

### §7 — Surface Redis error messages + P2 cleanups

At every `value.deinit(self.alloc)` site in `redis_client.zig` that's preceded by a logged error code, log `value.err` first so operators see the underlying server message (`READONLY` after failover, `BUSYGROUP` on consumer-group races, `WRONGTYPE`, etc.). P2 ride-alongs since RULE NLR applies on every queue file touched: compile-fold the XADD control argv constants; drop the 16 KiB plain-transport buffer default to 4 KiB to match the reference's `Pool.Options.read_buffer_size` (still env-overridable for callers needing larger).

### §8 — Bench + integration tests

`tests/bench/redis_xadd_concurrency_bench.zig` runs 8 producer threads, asserts post-pool throughput ≥4× pre-pool baseline.

**Methodology (pinned):**

- Local Redis on `localhost:6379` (no Upstash — masked by network latency).
- 8 producer threads, each XADDing 1000 events to distinct `bench:{thread_id}:events` streams (no shared key contention to avoid measuring Redis-server lock instead of client-side mutex).
- Two measurements, captured in separate commits during EXECUTE:
  1. **First commit** — pre-change baseline. Bench runs against today's single-mutex Client. Record ops/sec.
  2. **Second commit** — post-pool. Bench runs against the new pool-of-8 Client. Assert ops/sec ≥ 4× the baseline.
- Both numbers pasted into PR Session Notes with the git SHA of each measurement commit.

**CI handling:** bench is marked `// skip-in-ci` (no Redis service container in CI today). Local-only evidence — agent runs the bench on their dev machine, pastes output. If a future change adds a CI Redis container, flip the skip and let CI assert continuously.

Integration tests cover: pool acquire/release happy path; pool exhaustion (>max_idle in flight) creates new connection on the fly; release-as-broken closes the connection; subscriber with timeout fires; request timeout surfaces.

---

## Interfaces

**`src/queue/redis_pool.zig` public surface (locked shape — agent picks exact field types):**

```
pub const Pool = struct {
    pub fn init(alloc, cfg: Config, options: InitOptions) !Pool;
    pub fn deinit(self: *Pool) void;
    pub fn acquire(self: *Pool) !*Connection;
    pub fn release(self: *Pool, conn: *Connection, ok: bool) void;
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
```

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
| Pool deinit with in-flight connections | Server shutdown race | `Pool.deinit` walks idle only; in-flight Connections are owned by their caller and torn down when the caller returns. **Shutdown sequence (locked, in order):** (1) HTTP server stops accepting new requests (existing SIGTERM handler in `src/cmd/serve.zig`); (2) in-flight HTTP handlers drain (existing pattern); (3) watcher receives shutdown flag and exits its `XREADGROUP` loop, closing its dedicated connection; (4) per-zombie workers receive shutdown flag and close their dedicated XREADGROUP connections; (5) `Pool.deinit` walks the now-quiescent idle list and closes each connection. Steps 3–4 are existing infra (M42_003 era); step 5 is new for M69_004. |
| Env var typo (`REDIS_POOL_MAX_IDLE=abc`) | Operator misconfiguration | Parse error at boot; surface as `error.ConfigParseFailed`; server fails to start with a clear log line. |

---

## Invariants

1. `Client` holds no mutex around network I/O. `Pool` may hold a `std.Thread.Mutex` but only across linked-list pop/push (microseconds), never across `Connection.command`. Enforced by: (a) `grep -n "std.Thread.Mutex" src/queue/redis_client.zig` returns empty (the Client surface holds none); (b) code review of `src/queue/redis_pool.zig` confirms every `mutex.lock()` is paired with `mutex.unlock()` inside the same function body with no `Connection.command` / `Connection.dial` / `try` call between them.
2. Every `Pool.acquire` returns a `Connection` whose `Transport` is in a known state (connected; buffers reset) — enforced by `Connection.reset()` called inside `acquire` before return.
3. `Pool.release(conn, false)` always closes the connection — enforced by code review + test `test_release_broken_closes_conn`.
4. Every `setsockopt(SO_RCVTIMEO)` call has a matching `getsockopt` verification under debug builds — enforced by the existing audit pattern at `redis_pubsub.zig:79-96` (preserved in the unified subscriber).
5. `redis_pubsub.zig` no longer exists — enforced by `test ! -f src/queue/redis_pubsub.zig`.
6. The facade `redis.Subscriber` resolves to `redis_subscriber.Subscriber` — enforced by `grep -n "Subscriber" src/queue/redis.zig`.

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
| `test_xadd_throughput_4x_baseline` | Bench: 8 producer threads XADDing 1000 events each. Pool-of-8 ≥ 4× the pre-pool (single-mutex) baseline. (Local Redis; Upstash latency masks the multiplier — local is the apples-to-apples comparison.) |
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

---

## Acceptance Criteria

- [ ] Every Test Specification row passes — verify: `make test && make test-integration && make memleak`.
- [ ] Bench ≥4× throughput verified — verify: paste bench output in PR Session Notes.
- [ ] No `std.Thread.Mutex` in `redis_client.zig` — verify: `grep -c "std.Thread.Mutex" src/queue/redis_client.zig` returns `0`.
- [ ] `redis_pubsub.zig` deleted — verify: `test ! -f src/queue/redis_pubsub.zig`.
- [ ] All `redis_pubsub` references gone — verify: `grep -rn "redis_pubsub" src/` returns 0 hits.
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

# E2: contention bench
zig build bench-redis 2>&1 | tail -10
# expect: throughput post-pool >= 4x pre-pool baseline

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
- **Switch to `zig-okredis`'s pipelining model.** Audit explicit: half-pipelining is worse than no pipelining; pool-of-N likely renders pipelining moot.
- **Switch to RESP3.** Out of scope; the audit doesn't recommend it.
- **PostgreSQL LISTEN/NOTIFY as a Redis replacement.** Out of scope.
- **Adaptive pool sizing.** Fixed `max_idle` cap is sufficient; revisit only if bench post-landing shows pool-of-8 still leaves contention.
