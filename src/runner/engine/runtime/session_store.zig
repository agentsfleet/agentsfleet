//! Thread-safe `Session` store keyed by ExecutionId. File-as-struct: this
//! file IS `SessionStore`. Consumers `@import` it directly:
//!
//!     const SessionStore = @import("runtime/session_store.zig");
//!     var store = SessionStore.init(alloc);
//!     try store.put(session);

const std = @import("std");
const types = @import("../types.zig");
const Session = @import("../session.zig");

const SessionStore = @This();

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

test "SessionStore put/get/remove" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    const session = try alloc.create(Session);
    session.* = try Session.create(alloc, "/tmp/test", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});

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

test "SessionStore reapExpired returns 0 when no expired sessions" {
    const alloc = std.testing.allocator;
    var store = SessionStore.init(alloc);
    defer store.deinit();

    const session = try alloc.create(Session);
    session.* = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 300_000, .{}); // 5 minute lease — won't expire

    try store.put(session);
    try std.testing.expectEqual(@as(u32, 0), store.reapExpired());
    try std.testing.expectEqual(@as(usize, 1), store.activeCount());

    // Clean up.
    const removed = store.remove(session.execution_id);
    removed.?.destroy();
    alloc.destroy(removed.?);
}

test "SessionStore concurrent put/get from multiple threads" {
    const page = std.heap.page_allocator;
    var store = SessionStore.init(page);
    defer store.deinit();

    const Worker = struct {
        fn run(s: *SessionStore, a: std.mem.Allocator) void {
            for (0..10) |_| {
                const sess = a.create(Session) catch return;
                sess.* = Session.create(a, "/tmp/conc", .{
                    .trace_id = "t",
                    .zombie_id = "r",
                    .workspace_id = "w",
                    .session_id = "s",
                }, .{}, 30_000, .{}) catch return;
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
