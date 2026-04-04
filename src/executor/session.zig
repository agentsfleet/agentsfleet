//! Execution session lifecycle management.
//!
//! Each session represents a single execution (run) driven by the worker.
//! The executor owns the NullClaw runtime and sandbox enforcement within
//! the session boundary.

const std = @import("std");
const types = @import("types.zig");

const log = std.log.scoped(.executor_session);

pub const SessionError = error{
    SessionNotFound,
    SessionAlreadyExists,
    SessionCancelled,
    LeaseExpired,
};

pub const Session = struct {
    execution_id: types.ExecutionId,
    correlation: types.CorrelationContext,
    lease: types.LeaseState,
    resource_limits: types.ResourceLimits,
    workspace_path: []const u8,
    cancelled: std.atomic.Value(bool),
    arena: std.heap.ArenaAllocator,

    // Stage execution results (last completed stage).
    last_result: ?types.ExecutionResult = null,
    total_tokens: u64 = 0,
    total_wall_seconds: u64 = 0,
    stages_executed: u32 = 0,
    /// M27_001: highest peak memory across all stages (max, not sum).
    max_memory_peak_bytes: u64 = 0,
    /// M27_001: total CPU throttle time across all stages.
    total_cpu_throttled_ms: u64 = 0,

    pub fn create(
        alloc: std.mem.Allocator,
        workspace_path: []const u8,
        correlation: types.CorrelationContext,
        resource_limits: types.ResourceLimits,
        lease_timeout_ms: u64,
    ) Session {
        const execution_id = types.generateExecutionId();
        return .{
            .execution_id = execution_id,
            .correlation = correlation,
            .lease = .{
                .execution_id = execution_id,
                .last_heartbeat_ms = std.time.milliTimestamp(),
                .lease_timeout_ms = lease_timeout_ms,
            },
            .resource_limits = resource_limits,
            .workspace_path = workspace_path,
            .cancelled = std.atomic.Value(bool).init(false),
            .arena = std.heap.ArenaAllocator.init(alloc),
        };
    }

    pub fn cancel(self: *Session) void {
        self.cancelled.store(true, .release);
        const hex = types.executionIdHex(self.execution_id);
        log.info("session.cancelled execution_id={s}", .{&hex});
    }

    pub fn isCancelled(self: *const Session) bool {
        return self.cancelled.load(.acquire);
    }

    pub fn touchLease(self: *Session) void {
        self.lease.touch();
    }

    pub fn isLeaseExpired(self: *const Session) bool {
        return self.lease.isExpired();
    }

    pub fn recordStageResult(self: *Session, result: types.ExecutionResult) void {
        self.last_result = result;
        self.total_tokens += result.token_count;
        self.total_wall_seconds += result.wall_seconds;
        self.stages_executed += 1;
        // M27_001: track peak memory (max across stages) and CPU throttle (sum).
        if (result.memory_peak_bytes > self.max_memory_peak_bytes) {
            self.max_memory_peak_bytes = result.memory_peak_bytes;
        }
        self.total_cpu_throttled_ms += result.cpu_throttled_ms;
    }

    /// M27_001: Resource limits context for scoring normalization.
    pub const ResourceContext = struct {
        memory_limit_bytes: u64,
    };

    pub fn getUsage(self: *const Session) types.ExecutionResult {
        return .{
            .content = "",
            .token_count = self.total_tokens,
            .wall_seconds = self.total_wall_seconds,
            .exit_ok = self.last_result != null and (self.last_result.?.exit_ok),
            .failure = if (self.last_result) |r| r.failure else null,
            .memory_peak_bytes = self.max_memory_peak_bytes,
            .cpu_throttled_ms = self.total_cpu_throttled_ms,
        };
    }

    /// M27_001: Get resource limits for scoring normalization.
    pub fn getResourceContext(self: *const Session) ResourceContext {
        return .{
            .memory_limit_bytes = self.resource_limits.memory_limit_mb * 1024 * 1024,
        };
    }

    pub fn destroy(self: *Session) void {
        const hex = types.executionIdHex(self.execution_id);
        log.info("session.destroyed execution_id={s} stages={d} tokens={d}", .{ &hex, self.stages_executed, self.total_tokens });
        self.arena.deinit();
    }
};

/// Thread-safe session store keyed by ExecutionId.
pub const SessionStore = struct {
    mu: std.Thread.Mutex = .{},
    sessions: std.AutoHashMap(types.ExecutionId, *Session),
    alloc: std.mem.Allocator,

    pub fn init(alloc: std.mem.Allocator) SessionStore {
        return .{
            .sessions = std.AutoHashMap(types.ExecutionId, *Session).init(alloc),
            .alloc = alloc,
        };
    }

    pub fn put(self: *SessionStore, session: *Session) !void {
        self.mu.lock();
        defer self.mu.unlock();
        try self.sessions.put(session.execution_id, session);
    }

    pub fn get(self: *SessionStore, id: types.ExecutionId) ?*Session {
        self.mu.lock();
        defer self.mu.unlock();
        return self.sessions.get(id);
    }

    pub fn remove(self: *SessionStore, id: types.ExecutionId) ?*Session {
        self.mu.lock();
        defer self.mu.unlock();
        const entry = self.sessions.fetchRemove(id);
        return if (entry) |e| e.value else null;
    }

    pub fn activeCount(self: *SessionStore) usize {
        self.mu.lock();
        defer self.mu.unlock();
        return self.sessions.count();
    }

    /// Cancel and remove all sessions with expired leases.
    ///
    /// Uses a fixed-size stack buffer for removal IDs to avoid allocation.
    /// If >64 sessions expire simultaneously, the remainder are cancelled
    /// (flag set) but stay in the map — they'll be removed on the next
    /// reap cycle. This is safe: worker_concurrency per host is typically
    /// 4–8, so 64+ simultaneous orphans implies catastrophic failure.
    pub fn reapExpired(self: *SessionStore) u32 {
        self.mu.lock();
        defer self.mu.unlock();
        var it = self.sessions.iterator();
        var to_remove: [64]types.ExecutionId = undefined;
        var remove_count: usize = 0;

        while (it.next()) |entry| {
            if (entry.value_ptr.*.isLeaseExpired()) {
                entry.value_ptr.*.cancel();
                if (remove_count < to_remove.len) {
                    to_remove[remove_count] = entry.key_ptr.*;
                    remove_count += 1;
                }
            }
        }
        for (to_remove[0..remove_count]) |id| {
            if (self.sessions.fetchRemove(id)) |e| {
                e.value.destroy();
                self.alloc.destroy(e.value);
            }
        }
        return @intCast(remove_count);
    }

    pub fn deinit(self: *SessionStore) void {
        var it = self.sessions.iterator();
        while (it.next()) |entry| {
            entry.value_ptr.*.destroy();
            self.alloc.destroy(entry.value_ptr.*);
        }
        self.sessions.deinit();
    }
};

test "Session create and lifecycle" {
    var session = Session.create(std.testing.allocator, "/tmp/test", .{
        .trace_id = "trace-1",
        .run_id = "run-1",
        .workspace_id = "ws-1",
        .stage_id = "stage-1",
        .role_id = "echo",
        .skill_id = "echo",
    }, .{}, 30_000);
    defer session.destroy();

    try std.testing.expect(!session.isCancelled());
    try std.testing.expect(!session.isLeaseExpired());

    session.recordStageResult(.{ .content = "ok", .token_count = 100, .wall_seconds = 5, .exit_ok = true });
    try std.testing.expectEqual(@as(u64, 100), session.total_tokens);
    try std.testing.expectEqual(@as(u32, 1), session.stages_executed);

    session.cancel();
    try std.testing.expect(session.isCancelled());
}

test "SessionStore put/get/remove" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    const session = try alloc.create(Session);
    session.* = Session.create(alloc, "/tmp/test", .{
        .trace_id = "t",
        .run_id = "r",
        .workspace_id = "w",
        .stage_id = "s",
        .role_id = "echo",
        .skill_id = "echo",
    }, .{}, 30_000);

    try store.put(session);
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    const found = store.get(session.execution_id);
    try std.testing.expect(found != null);

    const removed = store.remove(session.execution_id);
    try std.testing.expect(removed != null);
    removed.?.destroy();
    alloc.destroy(removed.?);
    try std.testing.expectEqual(@as(usize, 0), store.activeCount());
}

test "SessionStore get returns null for unknown id" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    const unknown_id = types.generateExecutionId();
    try std.testing.expect(store.get(unknown_id) == null);
}

test "SessionStore remove returns null for unknown id" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    const unknown_id = types.generateExecutionId();
    try std.testing.expect(store.remove(unknown_id) == null);
}

test "Session getUsage returns cumulative results" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t", .run_id = "r", .workspace_id = "w",
        .stage_id = "s", .role_id = "echo", .skill_id = "echo",
    }, .{}, 30_000);
    defer session.destroy();

    session.recordStageResult(.{ .content = "", .token_count = 50, .wall_seconds = 2, .exit_ok = true });
    session.recordStageResult(.{ .content = "", .token_count = 30, .wall_seconds = 3, .exit_ok = true });

    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 80), usage.token_count);
    try std.testing.expectEqual(@as(u64, 5), usage.wall_seconds);
    try std.testing.expectEqual(@as(u32, 2), session.stages_executed);
}

test "Session touchLease refreshes lease" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t", .run_id = "r", .workspace_id = "w",
        .stage_id = "s", .role_id = "echo", .skill_id = "echo",
    }, .{}, 50); // 50ms lease
    defer session.destroy();

    // Touch before expiry.
    session.touchLease();
    try std.testing.expect(!session.isLeaseExpired());
}

test "SessionStore reapExpired returns 0 when no expired sessions" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    const session = try alloc.create(Session);
    session.* = Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t", .run_id = "r", .workspace_id = "w",
        .stage_id = "s", .role_id = "echo", .skill_id = "echo",
    }, .{}, 300_000); // 5 minute lease — won't expire

    try store.put(session);
    try std.testing.expectEqual(@as(u32, 0), store.reapExpired());
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    // Clean up.
    const removed = store.remove(session.execution_id);
    removed.?.destroy();
    alloc.destroy(removed.?);
}

// ── T5: Concurrency — multiple threads hitting SessionStore ──────────
test "SessionStore concurrent put/get from multiple threads" {
    const page = std.heap.page_allocator;
    var store = SessionStore.init(page);
    defer store.deinit();

    const Worker = struct {
        fn run(s: *SessionStore, a: std.mem.Allocator) void {
            for (0..10) |_| {
                const sess = a.create(Session) catch return;
                sess.* = Session.create(a, "/tmp/conc", .{
                    .trace_id = "t", .run_id = "r", .workspace_id = "w",
                    .stage_id = "s", .role_id = "echo", .skill_id = "echo",
                }, .{}, 30_000);
                s.put(sess) catch return;
            }
        }
    };

    var threads: [4]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = try std.Thread.spawn(.{}, Worker.run, .{ &store, page });
    }
    for (&threads) |*t| t.join();

    // All 40 sessions should be stored without data corruption.
    try std.testing.expectEqual(@as(usize, 40), store.activeCount());
}

// ── T7: Regression — Session struct fields stable ────────────────────
test "Session create initializes all fields correctly" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", .{
        .trace_id = "trace-abc", .run_id = "run-123", .workspace_id = "ws-456",
        .stage_id = "stg-1", .role_id = "echo", .skill_id = "echo",
    }, .{ .memory_limit_mb = 256, .cpu_limit_percent = 50 }, 5_000);
    defer session.destroy();

    try std.testing.expectEqualStrings("/tmp/ws", session.workspace_path);
    try std.testing.expectEqualStrings("trace-abc", session.correlation.trace_id);
    try std.testing.expectEqualStrings("run-123", session.correlation.run_id);
    try std.testing.expectEqual(@as(u64, 256), session.resource_limits.memory_limit_mb);
    try std.testing.expectEqual(@as(u64, 50), session.resource_limits.cpu_limit_percent);
    try std.testing.expectEqual(@as(u64, 5_000), session.lease.lease_timeout_ms);
    try std.testing.expectEqual(false, session.isCancelled());
    try std.testing.expect(session.last_result == null);
    try std.testing.expectEqual(@as(u64, 0), session.total_tokens);
    try std.testing.expectEqual(@as(u32, 0), session.stages_executed);
}

// ── T3: Error path — getUsage on session with no stages ──────────────
test "Session getUsage with no stages returns zero values" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t", .run_id = "r", .workspace_id = "w",
        .stage_id = "s", .role_id = "echo", .skill_id = "echo",
    }, .{}, 30_000);
    defer session.destroy();

    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 0), usage.token_count);
    try std.testing.expectEqual(@as(u64, 0), usage.wall_seconds);
    try std.testing.expectEqual(false, usage.exit_ok);
    try std.testing.expect(usage.failure == null);
}

// ── T2: Edge case — cancel is idempotent ─────────────────────────────
test "Session cancel is idempotent" {
    const alloc = std.testing.allocator;
    var session = Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t", .run_id = "r", .workspace_id = "w",
        .stage_id = "s", .role_id = "echo", .skill_id = "echo",
    }, .{}, 30_000);
    defer session.destroy();

    session.cancel();
    session.cancel(); // second cancel should not panic
    try std.testing.expect(session.isCancelled());
}
