# `src/queue/` Redis client audit

Read-only research artifact. Compares usezombie's hand-rolled Redis client against two third-party Zig Redis libraries, with the goal of seeding a follow-up implementation spec that fixes the M42_003 contention bottleneck.

**Hard constraints honoured:** no code edits in this dimension; no recommendation to switch libraries (the agent-stream / pub-sub code stays in-tree); every comparative claim cites a reference-lib `file:LN-LN`.

**Reference libraries (read-only):**
- `~/Projects/oss/redis.zig/` — karlseguin's `redis.zig` (RESP2, pool-based, blocking I/O via `zio`)
- `~/Projects/oss/zig-okredis/` — `zig-okredis` (RESP3, single-connection-per-client, pipelining via linked list of `Pending` records)

**usezombie surface under audit (read-only):**
- `src/queue/redis_client.zig` — `Client` struct (255 lines)
- `src/queue/redis_transport.zig` — `PlainTransport` + `TlsTransport` + `Transport` tagged union (197 lines)
- `src/queue/redis_pubsub.zig` — `Subscriber` (152 lines)
- `src/queue/redis_subscriber.zig` — second `Subscriber` (147 lines) — see Dimension 8 note
- `src/queue/redis_agent.zig` — agent stream ops (204 lines)
- `src/queue/redis_config.zig`, `redis_protocol.zig`, `redis_types.zig`, `redis.zig` facade

---

## Executive summary

1. **The single-mutex bottleneck is real and fixable by sharded locks, not by a pool.** Both reference libs avoid one big lock: `redis.zig` shards by acquiring a fresh connection from a pool (`Pool.zig:52-72`), `zig-okredis` splits the single connection into a write-side `wl` and read-side `rl` mutex with a `Pending` queue (`client.zig:22-23`, `client.zig:133-140`, `client.zig:142-235`). usezombie holds **one** `std.Thread.Mutex` for the full write+read round-trip (`redis_client.zig:8`, `:167-186`); every XADD/PUBLISH/XACK serializes against every other.
2. **No pool today, but adding one is the most direct win.** `Pool.zig` in `redis.zig` is 105 lines, a `SinglyLinkedList` of idle `Connection`s with `acquire`/`release` and a health flag (`Pool.zig:13`, `:52-72`, `:80-105`). Slotting an equivalent under `Client` is small surface change, big contention relief — and is strictly more practical than rewriting around `zig-okredis`'s pipelining model, which assumes the new `std.Io` async runtime usezombie hasn't adopted.
3. **The reconnect strategy is sound on the write phase and intentionally absent on the read phase; both libs agree.** usezombie reconnects only when the write fails (`redis_client.zig:179-184`) and explicitly does NOT retry post-write (the comment at `redis_client.zig:174-178` is correct). `redis.zig` makes the same distinction via `Protocol.isResumable` (`Protocol.zig:30-35`) — only `RedisError` (a server-side application error after a clean round-trip) is resumable; transport errors close the connection. This is one place where usezombie is already aligned with the reference; **do not regress this** when adding a pool.
4. **Per-command argv allocation in `xaddAgentEvent` is wasteful but bounded.** `redis_client.zig:129` allocates a fresh `[][]const u8` for every XADD; both reference libs avoid this — `redis.zig` uses a fixed `[max_keys + 1][]const u8` stack buffer for commands like DEL (`Connection.zig:143-146`), `zig-okredis` serializes directly via a comptime `CommandSerializer` (`client.zig:196`, `serializer.zig`). For XADD specifically, the variable-length payload (`payload_argv`) makes a stack buffer harder, but the outer 6 control slots (`"XADD"`, key, `"MAXLEN"`, `"~"`, `"10000"`, `"*"`) could live on the stack. Medium-priority cleanup.
5. **The package is one layering violation + one duplication-cleanup away from being extractable as `~/Projects/usezombie-queue-zig`.** Captain's posthog-zig precedent applies. The blockers today: (a) `redis_client.zig:122-155` (xaddAgentEvent imports `EventEnvelope` from `src/agent/`), (b) `redis_client.zig:253` and `redis_agent.zig:13` (`error_codes` import from `src/errors/`), (c) `logging.scoped(.redis_queue)` on every file (`log` module), and (d) two near-identical `Subscriber` types — `redis_subscriber.zig` (production SSE, `events_stream.zig:34`) vs `redis_pubsub.zig` (test harness + facade export, `test_harness_helpers.zig:48`). Move agent-shaped code out, dedup the subscribers, and the rest is a clean library. See "Package extraction viability" at the end.

---

## Per-dimension analysis

### 1. Allocation patterns

**usezombie today.**
- Per-command argv allocation: `redis_client.zig:129-130` (`alloc.alloc` + `defer alloc.free`) on every XADD.
- Response value lifetime: caller owns; `RespValue.deinit` recurses through the union (`redis_protocol.zig:10-23`). Each string field in a stream entry is a separate `alloc.dupe` (`redis_agent.zig:176-183`).
- Buffer sizing: 16 KiB read + 16 KiB write for plain transport (`redis_transport.zig:56-58`); for TLS, `min_buffer_len * 8` for the TLS write buffer and `min_buffer_len` for the others (`redis_transport.zig:115`). All statically sized.

**redis.zig.**
- Per-command argv: fixed-size stack buffer `[max_keys + 1][]const u8` (`Connection.zig:9`, `:143-146`) — caps at 64 keys but zero heap.
- Response value lifetime: caller passes a buffer for the small case (`Connection.zig:87`, `Protocol.zig:198-215`), or `readBulkStringResponseAlloc` for the heap case (`Protocol.zig:219-237`). `Value` union has a recursive `freeValue` (`Protocol.zig:156-169`) — same shape as usezombie.
- Buffer sizing: configurable via `Pool.Options.read_buffer_size` / `write_buffer_size` defaulting to 4096 (`Pool.zig:21-22`).

**zig-okredis.**
- Per-command argv: none — `CommandSerializer.serializeCommand` writes directly to the `Io.Writer` interface at `client.zig:196`. No intermediate `[][]const u8`.
- Response value lifetime: `RESP3.parseAlloc` returns a typed value; caller frees via the allocator (`parser.zig`).

**Verdict for usezombie.** P2. The argv-alloc on `xaddAgentEvent` is real but small (one alloc per XADD, 8 slots), and `EventEnvelope.encodeForXAdd` already returns a `[][]const u8` so the call site receives a slice. The buffer size of 16 KiB for plain transport is generous — could drop to 4 KiB to match `redis.zig`'s default, but that risks reframing XREADGROUP responses across multiple buffer fills with no measured benefit. **Leave alone unless bench shows allocator pressure.**

### 2. Connection pooling

**usezombie today.** No pool. `Client` owns one `Transport`. Every caller — every HTTP handler, every worker thread — shares the same `Client` via `*Client` and contends on the lock at `redis_client.zig:168-169`.

**redis.zig.** Pool is a `SinglyLinkedList` of idle `Connection`s, guarded by one `zio.Mutex`, with separate `acquire` / `release` (`Pool.zig:11-17`, `:52-72`, `:80-105`).
- **Lazy connect.** `acquire` returns from the idle list if non-empty; otherwise creates a new `Connection` **outside** the lock (`Pool.zig:65-72`). No eager warm-up. `max_idle` caps the pool size; over-cap connections are closed on release (`Pool.zig:90-98`).
- **Health flag.** `release(conn, ok: bool)` closes the connection when `ok=false` (`Pool.zig:80-87`) — that's how `Pipeline` returns broken connections (`Pipeline.zig:88-92` sets `self.healthy = false`).
- **Lifetime coupling.** `Pool.deinit` walks the idle list and closes every connection (`Pool.zig:44-50`); in-flight `Connection`s are owned by their caller via `acquire`.

**zig-okredis.** No pool — the client wraps a single `Io.Reader` + `Io.Writer` pair (`client.zig:19-23`). Pipelining inside one connection is the contention answer (see Dimension 3); for multi-conn you instantiate multiple `Client`s.

**Verdict for usezombie.** P0. Adopt `redis.zig`'s pool model with one adjustment: usezombie's worker pool is bounded (one worker per OS thread, sized at boot), so an `eager_min` option would let us preconnect to match the worker count and avoid the first-XADD reconnect storm at worker boot. **Concrete shape in "Recommendations" below.**

### 3. Concurrency model

**The headline.** usezombie's contention failure mode: every XADD/PUBLISH/XACK acquires `self.lock` for the **entire** write-then-read round-trip (`redis_client.zig:167-171`). With ~25 ms round-trip to Upstash (the M42_003 trace), max throughput per Client = 40 ops/sec/connection — and there is one connection.

**redis.zig.** No client-level lock. Concurrency = each thread acquires a connection from the pool; the pool's mutex is held only across the linked-list pop/push (`Pool.zig:53-63`, `:89-104`), not across the network call. The connection itself has no mutex — it's caller-owned during `acquire`/`release`.

**zig-okredis.** Single connection but two mutexes (`client.zig:22-23`):
- `wl` (write lock): held only while serializing the command and flushing (`client.zig:148-204`). The pending writer enqueues itself into `pending_tail` (`client.zig:154-161`), writes, then releases `wl` so the next writer can stream its bytes onto the same wire.
- `rl` (read lock): held while parsing the reply (`client.zig:207-235`). When the current reader finishes it signals the next `Pending`'s condition variable (`client.zig:227-234`).
- This is pipelining over a single TCP connection — multiple concurrent `send` calls overlap on the wire, replies are demultiplexed in FIFO order via the `Pending` linked list (`client.zig:133-140`).

**Verdict for usezombie.** P0 — adopt pool-of-connections (Dimension 2). P1 — consider split write/read locks inside each pooled connection ONLY if a follow-up bench shows pool-of-N still leaves contention on hot connections; the `Pending` machinery is non-trivial (75 lines at `client.zig:142-235` plus the `wl/rl/broken/cond` state) and the failure mode if it goes wrong is "reply N goes to caller M". A pool of N=worker-count connections likely renders this moot.

**Do not pipeline writes on a single connection without the read-side dispatch.** Half-pipelining is worse than no pipelining.

### 4. Fault tolerance + retry

**usezombie today.** Three layered behaviours, all correct:
- Write-phase retry: `commandUnlocked` catches the write error, reconnects, retries the write once (`redis_client.zig:179-184`). Comment at `:174-178` correctly identifies this as safe for any command because the server never saw it.
- Read-phase retry: explicitly disabled. Server may have already processed the write; replay would double-XADD or double-PUBLISH.
- Idempotent caller override: `readyCheck` (`redis_client.zig:71-94`) wraps `ping()` with its own reconnect-and-retry because PING is idempotent.

Health detection: SO_KEEPALIVE applied via `applyKeepalive` on every fresh dial (`redis_transport.zig:14-41`) — Linux gets idle/intvl/cnt knobs (30/10/3), macOS gets only `TCP_KEEPALIVE` (`:35-37`). Best-effort; failures swallowed at debug.

Error surfacing: command errors are logged with `error_codes.ERR_INTERNAL_OPERATION_FAILED` (`redis_client.zig:160`, `:144`) then mapped to `error.RedisCommandError` or `error.RedisXaddFailed`. Caller gets an opaque enum; the underlying Redis error message is freed at `:161` before it's surfaced.

**redis.zig.** Centralizes retry in `withConnection` (`Client.zig:66-110`):
- Wraps the whole `acquire → call → release` sequence in a `while (true)` loop.
- `attempts` counter, `retry_interval` sleep between attempts (`Client.zig:75-81`).
- Distinguishes resumable (Redis-level error) from non-resumable (transport) via `Protocol.isResumable` (`Protocol.zig:30-35`): resumable → connection goes back to pool, retry against the same one; non-resumable → connection is closed by `pool.release(conn, false)` and a fresh one is acquired.
- Retry default = 2 attempts (`Client.zig:27`), tunable.

**zig-okredis.** Has a `broken` flag (`client.zig:27`, `:36-50`) that poisons the client on AUTH/HELLO/serialization failure. No automatic reconnect — the client is a thin reader/writer wrapper, so the caller owns reconnect by rebuilding the `Client`.

**Verdict for usezombie.** P1 — when a pool lands, copy `redis.zig`'s `withConnection` retry-attempts pattern verbatim. P2 — surface the underlying Redis error message at `redis_client.zig:160-162` instead of dropping it; right now the operator sees `ERR_INTERNAL_OPERATION_FAILED` with no idea whether it was `READONLY` (failover), `BUSYGROUP`, `WRONGTYPE`, or anything else. The string is in `value.err` for one log emit and then gone.

Also worth lifting from zig-okredis: a `broken` gate (`client.zig:149-152`). After a failed AUTH or a TLS handshake that fails mid-init, the current `Client` can be in a half-built state — `connectFromUrl` runs `errdefer redis_config.deinitConfig` (`:18`) but the partially-initialized `transport` is `undefined` at that point. If `dialAndAuth` fails after `Client{}` is on the stack (`:20-21`), the caller's `defer client.deinit()` calls `transport.deinit` on `undefined`. Worth verifying: trace the failure paths in `connectFromUrl`.

### 5. Stability + reliability

**Invariants.** usezombie's `Client.deinit` (`redis_client.zig:26-29`) tears down transport then config. No drain step — Redis doesn't have postgres-style cursors that need closing, so this is fine. Both reference libs follow the same shape: `Pool.deinit` walks idle (`Pool.zig:44-50`), `okredis` has no `close()` and intentionally compileError's it (`client.zig:63-65`).

**Half-open detection.** All three libraries rely on TCP keepalive + write-error → reconnect. `redis.zig` adds connect / read / write timeouts as separate `zio.Timeout` knobs (`Connection.zig:22-25`). usezombie has no timeouts beyond SO_RCVTIMEO on the pub/sub subscriber (`redis_pubsub.zig:84-96`). A frozen Upstash proxy connection with keepalive ACKs in the kernel but no Redis traffic could leave a request blocked indefinitely. P1.

**TLS handshake failure.** usezombie logs at `:147` of `redis_transport.zig` and returns the error — but the partial TLS init has multi-stage allocations (`:109-122`) all wrapped in `errdefer`, so the failure path is clean. Spot-checked the chain: `socket_read_buffer`, `socket_write_buffer`, `tls_read_buffer`, `tls_write_buffer`, the two `create`d wrappers, and the `ca_bundle` all have matching `errdefer`s. **This is the cleanest part of the file.**

**Memleak guards.** `redis_protocol.zig:74-81` correctly tears down already-parsed array elements when a later parse fails — bug-free implementation of the standard pattern. `redis_agent.zig:188-194` does the same when a stream entry is missing required fields.

**Pub/sub disconnection.** Production SSE uses `redis_subscriber.nextMessage()` (`redis_subscriber.zig:88-111`), which returns `null` on `EndOfStream` / `ReadFailed`. The SSE handler at `src/http/handlers/agents/events_stream.zig:104-105` treats `null` as a clean disconnect from the Redis side and returns — so the browser reconnects via the SSE retry semantics. Test harness uses `redis_pubsub.readMessage()` (`redis_pubsub.zig:103-117`), which has the same null-on-error shape but additionally sets SO_RCVTIMEO=25_000 so a test budget loop can fire without messages. **Both paths are correct as designed; the duplication is what's wrong (see Dimension 8).** A unified subscriber with an optional read_timeout_ms preserves both behaviours.

### 6. Performance

**Per-command allocator pressure.** Covered in Dimension 1. The XADD argv heap alloc is the only avoidable one.

**Buffer reuse.** Both transports allocate buffers once at connect (`redis_transport.zig:56-58`, `:109-116`) and free at deinit. Reused across all commands. **Equivalent to both reference libs.**

**Unsafe fast paths.** redis.zig has none in the current source — older versions had `getUnsafe`; the audit-spec hint may be stale. zig-okredis offers `parseAlloc` vs `parse` (the latter is zero-alloc for fixed-size types) at `client.zig:69-81`. usezombie has no zero-alloc path; every bulk string is `alloc.dupe`'d (`redis_protocol.zig:59-61`).

**Prepared statements / pipelining.** None in usezombie. Both reference libs support it: `redis.zig` has `Pipeline.zig` (307 lines, queues commands and reads them all in `readResponse` after one flush, `Pipeline.zig:40-86`); `zig-okredis` pipelines via `Client.pipe` / `pipeAlloc` (`client.zig:119-131`).

**Where M42_003 bites.** XADD ratchets through one connection at ~40 ops/sec/connection (25 ms round-trip ceiling) regardless of how many CPU cores or worker threads exist. With a pool of 4 connections, ceiling = 160 ops/sec; pool of 8 = 320 ops/sec. **Pool size, not lock granularity, is the dial.**

**Verdict.** P0 = pool. P2 = prepared XADD argv (skip MAXLEN/`~`/`10000` per call if comptime'd). Pipelining = no, the agent workflow doesn't have batches of commands a pipeline would group; each XADD is independent and arrives on its own request.

### 7. Pooling return patterns

**redis.zig: `result.deinit` auto-releases.** Not quite — `Pipeline.deinit` calls `pool.release(self.conn, self.healthy)` (`Pipeline.zig:30-34`), so closing a pipeline returns the connection. For non-pipeline commands, the release happens inside `Client.withConnection` via `defer self.pool.release(conn, ok)` (`Client.zig:86`). The caller never sees `release` — they call `Client.get(...)` and the lifetime is hidden. This is the right shape.

**zig-okredis.** No pool; release is rebuilding the client.

**Verdict for usezombie.** When a pool lands, hide `acquire`/`release` from the call site. The current call sites are `try client.command(&.{...})`; they should remain `try client.command(&.{...})` after the pool change, with `acquire`/`release` wrapped internally. **Don't push the lifecycle to callers.**

### 8. What usezombie does that neither library handles

Confirmed stays:
- Per-agent stream consumer groups (`redis_agent.zig:50-66`).
- `XREADGROUP` / `XAUTOCLAIM` / `XACK` lifecycle (`redis_agent.zig:69-126`).
- Pub/sub subscriber with SO_RCVTIMEO heartbeat (`redis_pubsub.zig:84-96`, `:103-117`).
- Role-based ACL env vars `REDIS_URL` / `REDIS_URL_API` (`redis_types.zig`, `redis_config.zig`). The worker-role Redis URL retired with the worker substrate.

**One thing this audit surfaces that the spec didn't.** There are **two** subscriber implementations and they have split responsibilities:
- `src/queue/redis_subscriber.zig` — file-as-struct shape, used by **production** at `src/http/handlers/agents/events_stream.zig:34` (SSE handler). API: `nextMessage()` loops internally past non-message frames (PSUBSCRIBE pmessage, pong, subscribe count) at `redis_subscriber.zig:88-111`, returns `null` only on `EndOfStream`/`ReadFailed` (`:90-93`). No SO_RCVTIMEO — relies on the client (browser) to retry; the SSE handler at `events_stream.zig:104-105` treats `null` as "clean disconnect from Redis side" and returns.
- `src/queue/redis_pubsub.zig` — re-exported via the facade `redis.zig:11` as `redis.Subscriber`. Used by **test harnesses only**: `src/agent/test_harness_helpers.zig:48-51` and `src/agent/event_loop_harness_heartbeat_test.zig:61`. API: `readMessage()` (`redis_pubsub.zig:103-117`) sets SO_RCVTIMEO to 25_000 ms (`:79`, `:84-96`), returns `null` on the resulting `ReadFailed` so the test budget loop can advance.

Both files contain duplicated logic: identical `sendCommand` argv serializers (`redis_subscriber.zig:118-129` vs `redis_pubsub.zig:138-151`), near-identical `connectFromUrl` plumbing, two divergent `Message`/`PubSubMessage` struct definitions for the same wire shape. **Not dead code — duplicated code.** RULE NLR (touch-it-fix-it) applies whenever either is edited; the follow-up implementation spec should fold the two into a single `Subscriber` with an explicit `read_timeout_ms: ?u32` option (null = block forever, value = SO_RCVTIMEO + null-on-ReadFailed). Production passes `null`; test harness passes `25_000`.

---

## Concrete recommendations (ranked P0/P1/P2)

### P0 — Add a connection pool

Mirror `redis.zig`'s `Pool.zig` shape:

```zig
// src/queue/redis_pool.zig (sketch)
pub const Pool = struct {
    alloc: std.mem.Allocator,
    cfg: redis_config.Config,
    idle: std.SinglyLinkedList = .{},
    idle_count: usize = 0,
    max_idle: usize,
    eager_min: usize, // usezombie-specific: pre-warm to N=worker_count
    mutex: std.Thread.Mutex = .{},

    pub fn acquire(self: *Pool) !*Connection { ... }
    pub fn release(self: *Pool, conn: *Connection, ok: bool) void { ... }
};
```

Wrap acquire/release inside `Client.command` so all existing call sites are unchanged:

```zig
pub fn command(self: *Client, argv: []const []const u8) !RespValue {
    var attempts: usize = 0;
    while (true) : (attempts += 1) {
        const conn = try self.pool.acquire();
        var ok = false;
        defer self.pool.release(conn, ok);
        const result = conn.command(argv) catch |err| {
            if (Protocol.isResumable(err) and attempts == 0) { ok = true; continue; }
            return err;
        };
        ok = true;
        return result;
    }
}
```

Size: `pool` = ~120 lines; `Connection` = the current `Client` body minus the lock = ~200 lines. Net delta: +120, with the lock removed and the pool added.

Pin `eager_min` to `worker_count` from `src/cmd/serve.zig` so the API never pays first-request reconnect latency. Default `max_idle = 8`, configurable via `REDIS_POOL_MAX_IDLE` env.

Pin `Protocol.isResumable` equivalent: only `error.RedisCommandError` / `error.RedisXackFailed` / etc. (server-level) are resumable; all transport errors (`error.BrokenPipe`, `error.ConnectionResetByPeer`, `error.ReadFailed`) close the connection and let the pool create a fresh one.

### P0 — Fold the two subscribers into one

`redis_subscriber.zig` (production SSE) and `redis_pubsub.zig` (test harness + facade export) have ~85% byte-for-byte overlap and disagree only on `read_timeout_ms`. Unify on `redis_subscriber.zig`'s file-as-struct shape (`redis_subscriber.zig:15`), add an `InitOptions { read_timeout_ms: ?u32 = null }` argument, and:

- Production SSE: `connectFromEnv(alloc, .api, .{})` — block forever on `nextMessage` until message or clean disconnect.
- Test harness: `connectFromEnv(alloc, .worker, .{ .read_timeout_ms = 25_000 })` — SO_RCVTIMEO fires, `nextMessage` returns `null` so `drainFrames` can advance its budget loop.

Update the facade at `redis.zig:11` to re-export `redis_subscriber.Subscriber`, delete `redis_pubsub.zig`, retarget `test_harness_helpers.zig:48-51` and `event_loop_harness_heartbeat_test.zig:61` to the unified type. Net delta: -147 lines (`redis_pubsub.zig` gone) +20 lines (the option), no behaviour change at either call site.

**Why P0.** Duplication is the highest-leverage cleanup in this audit — and it's a hard prerequisite for package extraction (Captain's "posthog-zig" question below): you cannot ship a library with two near-identical types named `Subscriber` and `PubSubMessage` / `Message`.

### P1 — Add a hard read timeout to the request-path transport

Match `redis.zig`'s `read_timeout` knob (`Connection.zig:23`, `:50-51`). Today usezombie has SO_KEEPALIVE (`redis_transport.zig:14-41`) but no per-call timeout on the API connection. An Upstash proxy stall ≥ 60 s would block a worker thread indefinitely.

Reasonable default: `REDIS_REQUEST_TIMEOUT_MS = 5000`. Apply via `setsockopt(SO_RCVTIMEO)` like the pub/sub subscriber does (`redis_pubsub.zig:84-96`), but with a tight bound and emit `error.RedisRequestTimeout` so callers can decide between retry / surface-to-user.

### P1 — Surface Redis error message instead of dropping it

At `redis_client.zig:160-162`:

```zig
// today
log.err("command_error", .{ .cmd = ..., .error_code = ERR_INTERNAL_OPERATION_FAILED });
value.deinit(self.alloc);
return error.RedisCommandError;

// proposed
const msg = switch (value) { .err => |m| m, else => "" };
log.err("command_error", .{
    .cmd = if (argv.len > 0) argv[0] else "unknown",
    .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED,
    .redis_err = msg,
});
value.deinit(self.alloc);
return error.RedisCommandError;
```

The string is already in `value.err`; just log it before `value.deinit`. Same change at `:144`/`:148` for `xadd_agent_event_failed`.

### P1 — Verify `connectFromUrl`'s partial-init story

`redis_client.zig:16-24` allocates the `Config` and then calls `dialAndAuth`. If `dialAndAuth` errors after the `Client{}` literal at `:20-21` but before `:23`, the caller never sees a `Client` (the `try` propagates), so `defer client.deinit()` never fires. **This is actually fine.** But trace it once more under the pool refactor — moving connection construction into `Pool.acquire` shifts the failure surface.

### P2 — Compile-time-fold the XADD control argv

At `redis_client.zig:129-137`, the 6 control slots are constant across every XADD call. Define them once:

```zig
const XADD_CONTROL: [6][]const u8 = .{ "XADD", undefined, "MAXLEN", "~", "10000", "*" };
```

Slot 1 (stream_key) is per-call. Then `argv[0..6] = XADD_CONTROL; argv[1] = stream_key;` + memcpy payload — same heap alloc, but the compiler sees the literals once. **Negligible perf win.** Skip unless touching the file for an unrelated reason.

### P2 — Drop the 16 KiB buffers to 4 KiB

`redis_transport.zig:56-58` allocates 16 KiB read + 16 KiB write per `PlainTransport`. `redis.zig` defaults to 4 KiB (`Pool.zig:21-22`). For XADD/PUBLISH payloads (a few KiB at most), 4 KiB is enough; XREADGROUP responses can be larger but the std `Io.Reader` will refill. Pool of 8 → savings of ~192 KiB; not headline-worthy.

---

## Package extraction viability

Captain's question: can `src/queue/` move out into a standalone `usezombie-queue-zig` package alongside `posthog-zig`?

**Reference shape (posthog-zig).** `/Users/kishore/Projects/posthog-zig/build.zig.zon` declares `.name = .posthog`, `.dependencies = .{}`, `.minimum_zig_version = "0.16.0"`, with `paths` enumerating the public payload (`build.zig`, `src/`, `tests/`, and a hand-picked subset of `docs/`). No upstream `src/...` dependencies.

**usezombie-queue-zig as proposed.** Inventory of cross-boundary imports in `src/queue/`:

| File | Import | Crosses `src/queue/` boundary? |
|---|---|---|
| `redis_client.zig:253` | `@import("../errors/error_registry.zig")` | YES |
| `redis_client.zig:254` | `@import("../agent/event_envelope.zig")` | YES |
| `redis_client.zig:247` | `@import("log")` | YES (project-wide logger module) |
| `redis_agent.zig:13` | `@import("../errors/error_registry.zig")` | YES |
| `redis_agent.zig:9` | `@import("log")` | YES |
| `redis_transport.zig:3,5` | `@import("log")`, `@import("../errors/error_registry.zig")` | YES |
| `redis_pubsub.zig:5` | `@import("log")` | YES |
| `redis_subscriber.zig:142` | `@import("log")` | YES |

Three things to fix before a clean extraction:

1. **`EventEnvelope` is agent-shaped business logic in a "generic" Redis layer.** `xaddAgentEvent` (`redis_client.zig:122-155`) builds a `agent:{id}:events` stream key and decodes a domain envelope. **Move out of the package**: keep generic `xadd(stream_key, []FV)` in `redis_client.zig`, move the wrapper to `src/agent/redis_events.zig`. Same for `redis_agent.zig` — the stream-key formatting and EventEnvelope decoding belong on the agent side.

2. **`error_codes.ERR_INTERNAL_OPERATION_FAILED`** is only used at log sites — not part of the public surface. Two options: (a) drop the error_code embedding inside the package and let the caller log with its own scheme, or (b) define a package-local error registry (`queue_errors.zig`) and let the lead-repo logger remap. **Option (a)** is cleaner — log discipline (LOGGING_STANDARD §error-codes) is a lead-repo concern, not a library concern.

3. **`logging.scoped(.redis_queue)`.** The `log` module is a project-private module declared in `build.zig`. Package-extracted, the queue would use `std.log.scoped(.redis_queue)` directly. One-line change per file, eight files. The logger consumer (the lead repo) wires up `pub const std_options = .{ .logFn = ... }` to route std.log → the project logger.

**Conclusion.** Extraction is feasible — call it ~1 day of work — and produces a library with the same public footprint as posthog-zig: `Pool`, `Client`, `Subscriber`, `Config` types, `RespValue`. The carve-out makes the package usable for any Zig service Captain ships (queue infra is the same shape across products), and the lead repo's `src/agent/redis_events.zig` becomes the only place EventEnvelope semantics live.

**Recommendation: do the pool work IN-TREE first (P0 above) so the contention fix lands without waiting on a package split.** Then extract once the surface has stabilized — the package would inherit the pool, not race the pool.

---

## Code patterns to adopt (pseudocode summary)

| Source | Pattern | Where it lands |
|---|---|---|
| `redis.zig/src/Pool.zig:13-17` | `SinglyLinkedList` of idle connections + `idle_count` + `max_idle` cap | New `src/queue/redis_pool.zig` |
| `redis.zig/src/Pool.zig:52-72` | `acquire`: pop idle under lock, create-outside-lock on miss | Same |
| `redis.zig/src/Pool.zig:80-105` | `release(conn, ok)`: close on error, close on cap, otherwise push idle | Same |
| `redis.zig/src/Client.zig:66-110` | `withConnection`: retry loop, `Protocol.isResumable` gate | Refactor `Client.command` |
| `redis.zig/src/Protocol.zig:30-35` | `isResumable`: server-error → reuse, transport-error → close | New `src/queue/redis_errors.zig` |
| `redis.zig/src/Connection.zig:22-25,50-51` | `read_timeout` / `write_timeout` / `connect_timeout` as `setTimeout` calls | `Connection.connect` in the refactor |
| `zig-okredis/src/client.zig:22-23,27,149-152` | `broken` poison-pill flag — surface partial-init failure as a clean error | Optional; covered by pool's close-on-failure |

---

**End of audit.** Recommendations are ordered for incremental landing: P0 first (pool + subscriber dedup), then P1 (timeouts + error surfacing), then optional package extraction.
