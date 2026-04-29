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

test "Session.create deep-dupes the ExecutionPolicy into the session arena" {
    const alloc = std.testing.allocator;

    // Build a transient ExecutionPolicy on a separate arena that we deinit
    // BEFORE reading session.policy. If Session.create only borrowed the
    // slices, the reads below would dangle.
    var src_arena = std.heap.ArenaAllocator.init(alloc);
    const src = src_arena.allocator();

    const allow = try src.alloc([]const u8, 2);
    allow[0] = try src.dupe(u8, "api.machines.dev");
    allow[1] = try src.dupe(u8, "api.upstash.com");

    const tools = try src.alloc([]const u8, 1);
    tools[0] = try src.dupe(u8, "http_request");

    var fly_obj: std.json.ObjectMap = .init(src);
    try fly_obj.put(try src.dupe(u8, "api_token"), .{ .string = try src.dupe(u8, "FLY_TOKEN_VALUE") });

    var secrets_obj: std.json.ObjectMap = .init(src);
    try secrets_obj.put(try src.dupe(u8, "fly"), .{ .object = fly_obj });

    const policy = types.ExecutionPolicy{
        .network_policy = .{ .allow = allow },
        .tools = tools,
        .secrets_map = .{ .object = secrets_obj },
        .context = .{
            .tool_window = 25,
            .memory_checkpoint_every = 7,
            .stage_chunk_threshold = 0.8,
            .model = try src.dupe(u8, "claude-opus-4-7"),
        },
    };

    var session = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, policy);
    defer session.destroy();

    // Drop the source arena — anything still referencing src is dangling.
    src_arena.deinit();

    // Network allow list survived.
    try std.testing.expectEqual(@as(usize, 2), session.policy.network_policy.allow.len);
    try std.testing.expectEqualStrings("api.machines.dev", session.policy.network_policy.allow[0]);
    try std.testing.expectEqualStrings("api.upstash.com", session.policy.network_policy.allow[1]);

    // Tools allowlist survived.
    try std.testing.expectEqual(@as(usize, 1), session.policy.tools.len);
    try std.testing.expectEqualStrings("http_request", session.policy.tools[0]);

    // secrets_map JSON tree survived (key + nested field).
    const sm = session.policy.secrets_map.?;
    try std.testing.expect(sm == .object);
    const fly = sm.object.get("fly").?;
    try std.testing.expect(fly == .object);
    try std.testing.expectEqualStrings("FLY_TOKEN_VALUE", fly.object.get("api_token").?.string);

    // Context budget survived (model name was duped).
    try std.testing.expectEqual(@as(u32, 25), session.policy.context.tool_window);
    try std.testing.expectEqual(@as(u32, 7), session.policy.context.memory_checkpoint_every);
    try std.testing.expectApproxEqAbs(@as(f32, 0.8), session.policy.context.stage_chunk_threshold, 1e-6);
    try std.testing.expectEqualStrings("claude-opus-4-7", session.policy.context.model);
}

test "Session.create accepts an empty default ExecutionPolicy" {
    const alloc = std.testing.allocator;
    var session = try Session.create(alloc, "/tmp/ws", .{
        .trace_id = "t",
        .zombie_id = "r",
        .workspace_id = "w",
        .session_id = "s",
    }, .{}, 30_000, .{});
    defer session.destroy();

    try std.testing.expectEqual(@as(usize, 0), session.policy.network_policy.allow.len);
    try std.testing.expectEqual(@as(usize, 0), session.policy.tools.len);
    try std.testing.expect(session.policy.secrets_map == null);
    try std.testing.expectEqualStrings("", session.policy.context.model);
    try std.testing.expectEqual(@as(u32, 0), session.policy.context.tool_window); // 0 == "auto" sentinel
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
