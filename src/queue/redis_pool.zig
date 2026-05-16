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

pub const InitOptions = struct {
    max_idle: usize = 8,
    eager_min: usize = 2,
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
    acquire_timeouts_total: u64,
};

// === Fields ===

alloc: std.mem.Allocator,
cfg: redis_config.Config,
max_idle: usize,
eager_min: usize,
read_timeout_ms: ?u32,

idle: std.SinglyLinkedList = .{},
idle_count: usize = 0,
active_count: usize = 0,
mutex: std.Thread.Mutex = .{},

// Counters (cumulative across the pool's lifetime).
dials_total: u64 = 0,
overflow_dials_total: u64 = 0,
poisoned_connections_total: u64 = 0,
reconnects_total: u64 = 0,
forced_closes_total: u64 = 0,
acquire_timeouts_total: u64 = 0,

// === Lifecycle ===

/// Pool takes ownership of `cfg` and frees it in `deinit`.
pub fn init(alloc: std.mem.Allocator, cfg: redis_config.Config, options: InitOptions) !Pool {
    var pool: Pool = .{
        .alloc = alloc,
        .cfg = cfg,
        .max_idle = options.max_idle,
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
        conn.* = try Connection.init(alloc, &pool.cfg, .pooled);
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

/// Returns a connection in `.active` state with `role == .pooled`. The
/// dial (when idle list is empty) happens outside the mutex so other
/// pool users aren't blocked across a TLS handshake.
pub fn acquire(self: *Pool) !*Connection {
    self.mutex.lock();
    if (self.idle.popFirst()) |node| {
        self.idle_count -= 1;
        self.active_count += 1;
        self.mutex.unlock();
        return @fieldParentPtr("node", node);
    }
    const at_or_over_cap = self.active_count >= self.max_idle;
    self.active_count += 1;
    self.mutex.unlock();

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
    conn.* = try Connection.init(self.alloc, &self.cfg, .pooled);
    conn.applyReadTimeout(self.read_timeout_ms);
    dial_ok = true;

    self.mutex.lock();
    self.dials_total += 1;
    if (at_or_over_cap) self.overflow_dials_total += 1;
    self.mutex.unlock();
    return conn;
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
        self.mutex.unlock();
        conn.deinit();
        self.alloc.destroy(conn);
        return;
    }

    self.mutex.lock();
    if (self.idle_count >= self.max_idle) {
        self.forced_closes_total += 1;
        self.active_count -= 1;
        self.mutex.unlock();
        conn.deinit();
        self.alloc.destroy(conn);
        return;
    }
    self.idle.prepend(&conn.node);
    self.idle_count += 1;
    self.active_count -= 1;
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
const Connection = @import("redis_connection.zig");
const redis_config = @import("redis_config.zig");
