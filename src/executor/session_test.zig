//! Unit tests for `Session` (file-as-struct in session.zig). SessionStore
//! tests live in runtime/session_store.zig (alongside the type).

const std = @import("std");
const Session = @import("session.zig");
const types = @import("types.zig");

test "Session create and lifecycle" {
    var session = try Session.create(std.testing.allocator, "/tmp/test", .{
        .trace_id = "trace-1",
        .zombie_id = "run-1",
        .workspace_id = "ws-1",
        .session_id = "stage-1",
    }, .{}, 30_000, .{});
    defer session.destroy();

    try std.testing.expect(!session.isCancelled());
    try std.testing.expect(!session.isLeaseExpired());

    session.recordStageResult(.{ .content = "ok", .token_count = 100, .wall_seconds = 5, .exit_ok = true });
    try std.testing.expectEqual(@as(u64, 100), session.total_tokens);
    try std.testing.expectEqual(@as(u32, 1), session.stages_executed);

    session.cancel();
    try std.testing.expect(session.isCancelled());
}

test "Session getUsage returns cumulative results" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
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
    var session = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 50, .{}); // 50ms lease
    defer session.destroy();

    session.touchLease();
    try std.testing.expect(!session.isLeaseExpired());
}

test "Session create initializes all fields correctly" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "trace-abc",
        .zombie_id = "run-123",
        .workspace_id = "ws-456",
        .session_id = "stg-1",
    }, .{ .memory_limit_mb = 256, .cpu_limit_percent = 50 }, 5_000, .{});
    defer session.destroy();

    try std.testing.expectEqualStrings("/tmp/ws", session.workspace_path);
    try std.testing.expectEqualStrings("trace-abc", session.correlation.trace_id);
    try std.testing.expectEqualStrings("run-123", session.correlation.zombie_id);
    try std.testing.expectEqual(@as(u64, 256), session.resource_limits.memory_limit_mb);
    try std.testing.expectEqual(@as(u64, 50), session.resource_limits.cpu_limit_percent);
    try std.testing.expectEqual(@as(u64, 5_000), session.lease.lease_timeout_ms);
    try std.testing.expectEqual(false, session.isCancelled());
    try std.testing.expect(session.last_result == null);
    try std.testing.expectEqual(@as(u64, 0), session.total_tokens);
    try std.testing.expectEqual(@as(u32, 0), session.stages_executed);
}

test "Session getUsage with no stages returns zero values" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    const usage = session.getUsage();
    try std.testing.expectEqual(@as(u64, 0), usage.token_count);
    try std.testing.expectEqual(@as(u64, 0), usage.wall_seconds);
    try std.testing.expectEqual(false, usage.exit_ok);
    try std.testing.expect(usage.failure == null);
}

test "Session cancel is idempotent" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    session.cancel();
    session.cancel();
    try std.testing.expect(session.isCancelled());
}
