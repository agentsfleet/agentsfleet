const std = @import("std");
const pg = @import("pg");
const db = @import("../db/pool.zig");
const agents = @import("agents.zig");
const topology = @import("topology.zig");
const profile_resolver = @import("profile_resolver.zig");

fn openWorkerTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "WORKER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try db.parseUrl(arena.allocator(), url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

test "integration: workspace active profile is loaded for worker execution" {
    const db_ctx = (try openWorkerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_profile_versions (
            \\  profile_version_id TEXT PRIMARY KEY,
            \\  compiled_profile_json TEXT,
            \\  is_valid BOOLEAN NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspace_active_profile (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  profile_version_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const compiled =
        \\{
        \\  "profile_id": "acme-harness-v1",
        \\  "stages": [
        \\    {"stage_id":"plan","role":"planner","skill":"echo"},
        \\    {"stage_id":"implement","role":"implementer","skill":"scout"},
        \\    {"stage_id":"verify","role":"security","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_profile_versions (profile_version_id, compiled_profile_json, is_valid) VALUES ('pver_1', $1, TRUE)",
            .{compiled},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO workspace_active_profile (workspace_id, profile_version_id) VALUES ('ws_1', 'pver_1')",
            .{},
        );
        q.deinit();
    }

    var profile = (try profile_resolver.loadWorkspaceActiveProfile(std.testing.allocator, db_ctx.conn, "ws_1")) orelse return error.TestUnexpectedResult;
    defer profile.deinit();
    try std.testing.expectEqualStrings("acme-harness-v1", profile.profile_id);
    try std.testing.expectEqual(@as(usize, 3), profile.stages.len);
}

test "integration: worker profile fallback path returns null when no active binding" {
    const db_ctx = (try openWorkerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_profile_versions (
            \\  profile_version_id TEXT PRIMARY KEY,
            \\  compiled_profile_json TEXT,
            \\  is_valid BOOLEAN NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspace_active_profile (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  profile_version_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const none = try profile_resolver.loadWorkspaceActiveProfile(std.testing.allocator, db_ctx.conn, "ws_missing");
    try std.testing.expect(none == null);
}

test "integration: default topology roles resolve through registry" {
    var profile = try topology.defaultProfile(std.testing.allocator);
    defer profile.deinit();

    for (profile.stages) |stage| {
        try std.testing.expect(agents.resolveRole(stage.role_id, stage.skill_id) != null);
    }
}
