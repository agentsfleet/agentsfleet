//! StreamRegistry — the owner of this instance's live SSE streams.
//!
//! Replaces the bare in-flight counter: every live stream is an entry
//! `{workspace_id, fleet_id, started_ms, client fd}`, so the instance can
//! (a) admit against the cap and keep the gauge from one source of truth,
//! (b) DRAIN at shutdown — `shutdown(2)` each client socket so stream
//! threads' next write fails fast instead of lingering on detached threads,
//! and (c) list live streams for the operator plane.
//!
//! fd lifecycle safety: the stream thread deregisters BEFORE closing its
//! socket, and `drain` only touches fds of entries still in the map (under
//! the mutex) — so a drain can never `shutdown` a closed-and-reused
//! descriptor. Entries created on the request thread carry no fd until the
//! detached thread attaches one; a drain in that window skips them (they
//! exit through the hub's close path instead).

const Self = @This();

alloc: std.mem.Allocator,
io: std.Io,
mutex: std.Io.Mutex = .init,
entries: std.AutoHashMapUnmanaged(u64, Entry) = .empty,
next_id: u64 = 1,
draining: bool = false,

const FD_UNATTACHED: std.posix.fd_t = -1;

/// awaitEmpty's bound on the stream threads' deregistration, so callers can
/// tear down whatever the streams borrow.
const AWAIT_EMPTY_MAX_MS: u64 = 5_000;
const AWAIT_EMPTY_POLL_MS: u64 = 50;

const Entry = struct {
    workspace_id: []u8,
    fleet_id: []u8,
    started_ms: i64,
    fd: std.posix.fd_t = FD_UNATTACHED,
};

/// Operator-plane listing row — the client fd stays internal.
pub const ListedStream = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    started_ms: i64,
};

pub fn init(alloc: std.mem.Allocator, io: std.Io) Self {
    return .{ .alloc = alloc, .io = io };
}

/// Frees remaining entries. Call after `drain()` (or after every stream is
/// known to have deregistered) — a live stream thread still holding its id
/// must never race this.
pub fn deinit(self: *Self) void {
    var it = self.entries.valueIterator();
    while (it.next()) |entry| self.freeEntry(entry.*);
    self.entries.deinit(self.alloc);
}

/// Claim a stream slot: check-and-insert under one lock (no over-claim
/// wobble). Null = at capacity or draining → the caller sheds.
pub fn tryRegister(self: *Self, workspace_id: []const u8, fleet_id: []const u8, started_ms: i64, max: u32) error{OutOfMemory}!?u64 {
    const ws = try self.alloc.dupe(u8, workspace_id);
    errdefer self.alloc.free(ws);
    const zid = try self.alloc.dupe(u8, fleet_id);
    errdefer self.alloc.free(zid);

    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    if (self.draining or self.entries.count() >= max) {
        self.alloc.free(ws);
        self.alloc.free(zid);
        return null;
    }
    const id = self.next_id;
    self.next_id += 1;
    try self.entries.put(self.alloc, id, .{
        .workspace_id = ws,
        .fleet_id = zid,
        .started_ms = started_ms,
    });
    metrics.setSseInFlightStreams(@intCast(self.entries.count()));
    return id;
}

/// The detached stream thread attaches its client socket once it owns it.
pub fn attachFd(self: *Self, id: u64, fd: std.posix.fd_t) void {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    if (self.entries.getPtr(id)) |entry| entry.fd = fd;
}

/// Release a slot. Idempotent — a double release is a no-op, never an
/// underflow. The caller must deregister BEFORE closing the entry's fd.
pub fn deregister(self: *Self, id: u64) void {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    const kv = self.entries.fetchRemove(id) orelse return;
    self.freeEntry(kv.value);
    metrics.setSseInFlightStreams(@intCast(self.entries.count()));
}

pub fn count(self: *Self) usize {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    return self.entries.count();
}

/// Shutdown drain, step one of the choreography: reject new streams and
/// `shutdown(2)` every attached client socket so a stream thread blocked in
/// a WRITE fails fast. A stream parked in its subscription pop (a futex
/// wait, not a socket read) is NOT woken by this — that wake is the hub's
/// close broadcast; callers run drain() → hub.stop() → awaitEmpty().
pub fn drain(self: *Self) void {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    self.draining = true;
    const live = self.entries.count();
    var it = self.entries.valueIterator();
    while (it.next()) |entry| {
        if (entry.fd == FD_UNATTACHED) continue;
        // best-effort libc shutdown by fd (the wake-a-blocked-peer
        // pattern the runner's call deadline uses); a racing client
        // disconnect already woke the thread anyway
        _ = std.c.shutdown(entry.fd, std.c.SHUT.RDWR);
    }
    if (live > 0) log.debug("drain_started", .{ .live_streams = live });
}

/// Step three: block until every woken stream thread has deregistered, so
/// callers can tear down what the streams borrow (hub, registry). Waits in
/// bounded rounds; each incomplete round logs the straggler count and
/// re-arms — the storage is NEVER freed under a live stream thread (a loud
/// hang that systemd escalates beats a use-after-free in a wedged thread).
pub fn awaitEmpty(self: *Self) void {
    _ = self.awaitEmptyRounds(AWAIT_EMPTY_MAX_MS);
}

/// Testable core of `awaitEmpty`: waits in rounds of `round_ms`, returning
/// the number of incomplete (logged, re-armed) rounds once empty.
pub fn awaitEmptyRounds(self: *Self, round_ms: u64) u32 {
    var rounds: u32 = 0;
    while (true) {
        var waited_ms: u64 = 0;
        while (self.count() > 0 and waited_ms < round_ms) : (waited_ms += AWAIT_EMPTY_POLL_MS) {
            common.sleepNanos(AWAIT_EMPTY_POLL_MS * std.time.ns_per_ms);
        }
        if (self.count() == 0) return rounds;
        rounds += 1;
        log.warn("drain_incomplete", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .live_streams = self.count(), .rounds = rounds });
    }
}

/// Listing rows duped into `alloc`. Callers today pass a request arena (free
/// with the arena, or not at all), but OOM unwinds the partial rows either
/// way — a future general-purpose-allocator caller inherits no leak.
pub fn listAlloc(self: *Self, alloc: std.mem.Allocator) error{OutOfMemory}![]ListedStream {
    self.mutex.lockUncancelable(self.io);
    defer self.mutex.unlock(self.io);
    var rows = try alloc.alloc(ListedStream, self.entries.count());
    errdefer alloc.free(rows);
    var i: usize = 0;
    errdefer for (rows[0..i]) |row| {
        alloc.free(row.workspace_id);
        alloc.free(row.fleet_id);
    };
    var it = self.entries.valueIterator();
    while (it.next()) |entry| : (i += 1) {
        const workspace_id = try alloc.dupe(u8, entry.workspace_id);
        errdefer alloc.free(workspace_id);
        rows[i] = .{
            .workspace_id = workspace_id,
            .fleet_id = try alloc.dupe(u8, entry.fleet_id),
            .started_ms = entry.started_ms,
        };
    }
    return rows;
}

fn freeEntry(self: *Self, entry: Entry) void {
    self.alloc.free(entry.workspace_id);
    self.alloc.free(entry.fleet_id);
}

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const metrics = @import("../observability/metrics.zig");
const log = logging.scoped(.stream_registry);

test {
    _ = @import("stream_registry_test.zig");
}
