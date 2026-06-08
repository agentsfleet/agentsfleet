//! Pool of `Connection` instances for request-path Redis commands.
//!
//! Mirrors `~/Projects/oss/redis.zig/src/Pool.zig`: intrusive
//! `SinglyLinkedList` of idle connections, `max_idle` cap, `eager_min`
//! preconnect. The mutex is held ONLY across list pop/push (microseconds);
//! dialing happens outside the lock (Invariant 1).
//!
//! Dedicated and subscriber connections are NOT routed through this pool —
//! see spec §1 "Connection topology". `Pool.release` rejects non-pooled
//! roles (Invariants 8, 9).
//!
//! Slice 1 lays the shape down. The Client façade rewire + retry loop land
//! in slice 3; `/metrics` export of `PoolStats` lands in slice 7.

const Pool = @This();

/// Sentinel for `max_active`: zero disables the hard ceiling, restoring the
/// metric-only overflow behavior (active count may grow unbounded; the cap
/// check falls back to `max_idle` purely to keep `overflow_dials_total` a
/// live producer). Any positive value is a hard ceiling enforced with
/// condvar backpressure.
const max_active_unbounded: usize = 0;

/// Default acquire-wait budget (milliseconds) when a hard `max_active`
/// ceiling is set but no explicit budget is supplied. A saturated pool
/// blocks an acquirer up to this long before surfacing `AcquireTimeout`.
const default_acquire_timeout_ms: u32 = 5_000;

/// Bounded-poll slice for `waitForActiveSlot`. Zig 0.16 dropped `timedWait`
/// from the Io sync primitives, so a saturated acquirer sleeps in short
/// slices (re-checking the predicate each wake) instead of blocking on a
/// timed condvar. ~2ms keeps wakeup latency low without busy-spinning.
const POLL_INTERVAL_NS: u64 = 2 * std.time.ns_per_ms;

pub const InitOptions = struct {
    max_idle: usize = 8,
    eager_min: usize = 2,
    /// Hard ceiling on `active_count`. `max_active_unbounded` (0) keeps the
    /// historical metric-only behavior (overflow grows the pool). A positive
    /// value caps concurrency: an acquirer that would exceed it blocks on the
    /// `not_full` condvar up to `acquire_timeout_ms`, then returns
    /// `error.AcquireTimeout`. This is the real producer of
    /// `acquire_timeouts_total`.
    max_active: usize = max_active_unbounded,
    /// Acquire-wait budget in milliseconds, applied only when `max_active` is
    /// a hard ceiling. Defaults to `default_acquire_timeout_ms`.
    acquire_timeout_ms: u32 = default_acquire_timeout_ms,
    /// `SO_RCVTIMEO` applied to every pooled Connection's transport (spec
    /// §6). Null = no request-path timeout (legacy block-forever). Boot
    /// reads `REDIS_REQUEST_TIMEOUT_MS` in `serve.zig` and threads through.
    read_timeout_ms: ?u32 = null,
};

pub const PoolStats = struct {
    // Utilization (snapshot)
    active: usize,
    idle: usize,
    dials_total: u64,
    overflow_dials_total: u64,
    acquire_wait_ns_p99: u64,

    // Pathology (cumulative, never reset)
    poisoned_connections_total: u64,
    reconnects_total: u64,
    forced_closes_total: u64,
    /// Acquire calls that exhausted their wait budget against a hard
    /// `max_active` ceiling and returned `error.AcquireTimeout`. Stays 0
    /// when `max_active` is `max_active_unbounded` (no ceiling → acquire
    /// never blocks).
    acquire_timeouts_total: u64,
};

// === Fields ===

alloc: std.mem.Allocator,
/// Backs the bounded-poll sleep in `waitForActiveSlot` (Zig 0.16 has no
/// `timedWait`). Set in `init`, never mutated.
io: std.Io,
cfg: redis_config.Config,
max_idle: usize,
/// Hard ceiling on `active_count`; `max_active_unbounded` disables it.
max_active: usize,
acquire_timeout_ms: u32,
eager_min: usize,
read_timeout_ms: ?u32,

idle: std.SinglyLinkedList = .{},
idle_count: usize = 0,
active_count: usize = 0,
mutex: common.Mutex = .{},
/// Signalled by `release` whenever `active_count` drops, waking an acquirer
/// that is blocked on a full `max_active` ceiling. Paired with `mutex`.
not_full: common.Condition = .{},

// Counters (cumulative across the pool's lifetime).
dials_total: u64 = 0,
overflow_dials_total: u64 = 0,
poisoned_connections_total: u64 = 0,
reconnects_total: u64 = 0,
forced_closes_total: u64 = 0,
// Bumped by `acquire` when a hard `max_active` ceiling makes it wait
// past `acquire_timeout_ms`; stays 0 while `max_active` is unbounded.
acquire_timeouts_total: u64 = 0,

// === Lifecycle ===

/// Pool takes ownership of `cfg` and frees it in `deinit`. `io` backs the
/// bounded-poll acquire wait.
pub fn init(io: std.Io, alloc: std.mem.Allocator, cfg: redis_config.Config, options: InitOptions) !Pool {
    var pool: Pool = .{
        .alloc = alloc,
        .io = io,
        .cfg = cfg,
        .max_idle = options.max_idle,
        .max_active = options.max_active,
        .acquire_timeout_ms = options.acquire_timeout_ms,
        .eager_min = options.eager_min,
        .read_timeout_ms = options.read_timeout_ms,
    };
    errdefer redis_config.deinitConfig(alloc, pool.cfg);

    var dialed: usize = 0;
    errdefer while (pool.idle.popFirst()) |node| {
        const conn: *Connection = @fieldParentPtr("node", node);
        conn.deinit();
        alloc.destroy(conn);
        dialed -= 1;
    };

    while (dialed < options.eager_min) : (dialed += 1) {
        const conn = try alloc.create(Connection);
        errdefer alloc.destroy(conn);
        conn.* = try Connection.init(io, alloc, &pool.cfg, .pooled);
        conn.applyReadTimeout(pool.read_timeout_ms);
        pool.idle.prepend(&conn.node);
        pool.idle_count += 1;
        pool.dials_total += 1;
    }

    return pool;
}

pub fn deinit(self: *Pool) void {
    while (self.idle.popFirst()) |node| {
        const conn: *Connection = @fieldParentPtr("node", node);
        conn.deinit();
        self.alloc.destroy(conn);
    }
    redis_config.deinitConfig(self.alloc, self.cfg);
}

// === Acquire / Release ===

/// Outcome of reserving an active slot. `reused` is non-null when an idle
/// connection became available (no dial needed); otherwise the caller must
/// dial and `at_or_over_cap` records whether the reservation crossed the
/// overflow threshold (drives `overflow_dials_total`).
const SlotReservation = struct {
    reused: ?*Connection,
    at_or_over_cap: bool,
};

/// Returns a connection in `.active` state with `role == .pooled`. The
/// dial (when idle list is empty) happens outside the mutex so other
/// pool users aren't blocked across a TLS handshake. When a hard
/// `max_active` ceiling is set and the pool is saturated, the call blocks
/// on `not_full` up to `acquire_timeout_ms`, then returns
/// `error.AcquireTimeout`.
pub fn acquire(self: *Pool) !*Connection {
    self.mutex.lock();
    const slot = self.reserveActiveSlot() catch |err| {
        self.mutex.unlock();
        return err;
    };
    self.mutex.unlock();
    if (slot.reused) |conn| return conn;
    const at_or_over_cap = slot.at_or_over_cap;

    // Every failure path below must decrement `active_count`, otherwise an
    // OOM during dial permanently inflates the pool's active count and
    // starves overflow-decision logic. The errdefer block is the single
    // chokepoint that fires on any `try`/`catch |err| return err` below.
    var dial_ok = false;
    errdefer if (!dial_ok) {
        self.mutex.lock();
        self.active_count -= 1;
        self.mutex.unlock();
    };

    const conn = try self.alloc.create(Connection);
    errdefer self.alloc.destroy(conn);
    conn.* = try Connection.init(self.io, self.alloc, &self.cfg, .pooled);
    conn.applyReadTimeout(self.read_timeout_ms);
    dial_ok = true;

    self.mutex.lock();
    self.dials_total += 1;
    if (at_or_over_cap) self.overflow_dials_total += 1;
    self.mutex.unlock();
    return conn;
}

/// Reserve a slot for the caller. MUST be called with `mutex` held and
/// returns with it still held (no I/O happens under the lock). Pops an idle
/// connection when one exists; otherwise reserves an active slot, blocking
/// on a hard `max_active` ceiling. Increments `active_count` for the slot it
/// hands out (idle or fresh-dial) so the count reflects the in-flight conn.
fn reserveActiveSlot(self: *Pool) error{AcquireTimeout}!SlotReservation {
    if (self.idle.popFirst()) |node| {
        self.idle_count -= 1;
        self.active_count += 1;
        return .{ .reused = @as(*Connection, @fieldParentPtr(POOL_NODE_FIELD, node)), .at_or_over_cap = false };
    }
    if (self.max_active != max_active_unbounded) {
        if (try self.waitForActiveSlot()) |conn| {
            return .{ .reused = conn, .at_or_over_cap = false };
        }
    }
    const at_or_over_cap = self.active_count >= self.max_idle;
    self.active_count += 1;
    return .{ .reused = null, .at_or_over_cap = at_or_over_cap };
}

/// Block until an active slot is free under a hard `max_active` ceiling, the
/// idle list refills, or the wait budget expires. MUST hold `mutex`; returns
/// with it held. A non-null result is a reused idle conn whose `active_count`
/// is already incremented; null means a fresh dial is owed. The deadline is
/// fixed up front so spurious wakes can't extend the budget.
fn waitForActiveSlot(self: *Pool) error{AcquireTimeout}!?*Connection {
    const budget_ns = @as(u64, self.acquire_timeout_ms) * std.time.ns_per_ms;
    const deadline_ns = clock.nowNanos() + @as(i128, budget_ns);
    while (self.active_count >= self.max_active and self.idle_count == 0) {
        const now_ns = clock.nowNanos();
        if (now_ns >= deadline_ns) {
            self.acquire_timeouts_total += 1;
            return error.AcquireTimeout;
        }
        const remaining_ns = @as(u64, @intCast(deadline_ns - now_ns));
        // Bounded poll: Zig 0.16 has no `timedWait`, so drop the lock, sleep a
        // short slice (capped by the remaining budget), then re-take and let the
        // while-condition re-check the deadline + active/idle predicate. `release`
        // still signals `not_full` (inert here) so a real timed condvar wait can
        // be restored verbatim once an Io exposes one.
        const slice_ns = @min(remaining_ns, POLL_INTERVAL_NS);
        self.mutex.unlock();
        std.Io.sleep(self.io, std.Io.Duration.fromNanoseconds(@intCast(slice_ns)), .awake) catch {
            // Cancellation (shutdown) aborts the acquire; re-lock to honor the
            // held-mutex contract before surfacing the timeout.
            self.mutex.lock();
            return error.AcquireTimeout;
        };
        self.mutex.lock();
    }
    if (self.idle.popFirst()) |node| {
        self.idle_count -= 1;
        self.active_count += 1;
        return @as(*Connection, @fieldParentPtr(POOL_NODE_FIELD, node));
    }
    return null;
}

/// Return a Connection to the pool. `ok=false` closes the connection
/// (transport error or poisoned state). `ok=true` returns it to idle iff
/// `idle_count < max_idle`; otherwise closes (over-cap overflow).
pub fn release(self: *Pool, conn: *Connection, ok: bool) void {
    std.debug.assert(conn.role == .pooled);

    if (!ok or conn.state != .active) {
        self.mutex.lock();
        if (conn.state == .poisoned) self.poisoned_connections_total += 1;
        self.active_count -= 1;
        // Freed an active slot. The `not_full.signal()` below is inert — nothing waits
        // on `not_full` (waitForActiveSlot poll-sleeps; Zig 0.16 has no timed condvar
        // wait). Kept for the timed-wait restore — do not prune the condvar.
        self.not_full.signal();
        self.mutex.unlock();
        conn.deinit();
        self.alloc.destroy(conn);
        return;
    }

    self.mutex.lock();
    if (self.idle_count >= self.max_idle) {
        self.forced_closes_total += 1;
        self.active_count -= 1;
        self.not_full.signal(); // inert — see the active-slot note above
        self.mutex.unlock();
        conn.deinit();
        self.alloc.destroy(conn);
        return;
    }
    self.idle.prepend(&conn.node);
    self.idle_count += 1;
    self.active_count -= 1;
    // Slot freed and an idle conn is now parked. `not_full.signal()` is inert —
    // see the active-slot note above; kept for the timed-wait restore.
    self.not_full.signal();
    self.mutex.unlock();
}

// === Observability ===

/// Record a fresh dial performed by the Client retry layer after a
/// transport-level (non-resumable) failure. Pool exports the counter
/// via `stats()`; the Client retry loop is the only producer.
pub fn recordReconnect(self: *Pool) void {
    self.mutex.lock();
    defer self.mutex.unlock();
    self.reconnects_total += 1;
}

pub fn stats(self: *Pool) PoolStats {
    self.mutex.lock();
    defer self.mutex.unlock();
    return .{
        .active = self.active_count,
        .idle = self.idle_count,
        .dials_total = self.dials_total,
        .overflow_dials_total = self.overflow_dials_total,
        .acquire_wait_ns_p99 = 0, // sliding histogram wires in slice 7 alongside /metrics
        .poisoned_connections_total = self.poisoned_connections_total,
        .reconnects_total = self.reconnects_total,
        .forced_closes_total = self.forced_closes_total,
        .acquire_timeouts_total = self.acquire_timeouts_total,
    };
}

// === Imports ===

const std = @import("std");
const common = @import("common");
const clock = @import("common").clock;
const Connection = @import("redis_connection.zig");
const redis_config = @import("redis_config.zig");
const POOL_NODE_FIELD = "node";
