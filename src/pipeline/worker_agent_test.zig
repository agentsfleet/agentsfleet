const std = @import("std");
const pg = @import("pg");
const db = @import("../db/pool.zig");
const agents = @import("agents.zig");
const topology = @import("topology.zig");
const profile_resolver = @import("profile_resolver.zig");

fn openWorkerTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    // check-pg-drain: ok — no conn.query() here; checker misattributes test-block queries
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
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_config_versions (
            \\  config_version_id TEXT PRIMARY KEY,
            \\  compiled_profile_json TEXT,
            \\  is_valid BOOLEAN NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspace_active_config (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  config_version_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const compiled =
        \\{
        \\  "agent_id": "acme-harness-v1",
        \\  "stages": [
        \\    {"stage_id":"plan","role":"planner","skill":"echo"},
        \\    {"stage_id":"implement","role":"implementer","skill":"scout"},
        \\    {"stage_id":"verify","role":"security","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_config_versions (config_version_id, compiled_profile_json, is_valid) VALUES ('0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98', $1, TRUE)",
            .{compiled},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO workspace_active_config (workspace_id, config_version_id) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98')",
            .{},
        );
        q.deinit();
    }

    var profile = (try profile_resolver.loadWorkspaceActiveProfile(std.testing.allocator, db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11")) orelse return error.TestUnexpectedResult;
    defer profile.deinit();
    try std.testing.expectEqualStrings("acme-harness-v1", profile.agent_id);
    try std.testing.expectEqual(@as(usize, 3), profile.stages.len);
}

test "integration: worker profile fallback path returns null when no active binding" {
    const db_ctx = (try openWorkerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_config_versions (
            \\  config_version_id TEXT PRIMARY KEY,
            \\  compiled_profile_json TEXT,
            \\  is_valid BOOLEAN NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspace_active_config (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  config_version_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const none = try profile_resolver.loadWorkspaceActiveProfile(std.testing.allocator, db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f19");
    try std.testing.expect(none == null);
}

test "integration: switching active profile changes worker-resolved profile deterministically" {
    const db_ctx = (try openWorkerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_config_versions (
            \\  config_version_id TEXT PRIMARY KEY,
            \\  compiled_profile_json TEXT,
            \\  is_valid BOOLEAN NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspace_active_config (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  config_version_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const profile_a =
        \\{
        \\  "agent_id": "acme-harness-v1",
        \\  "stages": [
        \\    {"stage_id":"plan","role":"planner","skill":"echo"},
        \\    {"stage_id":"implement","role":"implementer","skill":"scout"},
        \\    {"stage_id":"verify","role":"security","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;

    const profile_b =
        \\{
        \\  "agent_id": "acme-harness-v2",
        \\  "stages": [
        \\    {"stage_id":"plan","role":"planner","skill":"echo"},
        \\    {"stage_id":"security-review","role":"security","skill":"warden"},
        \\    {"stage_id":"implement","role":"implementer","skill":"scout"},
        \\    {"stage_id":"verify","role":"security","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}
        \\  ]
        \\}
    ;

    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_config_versions (config_version_id, compiled_profile_json, is_valid) VALUES ('0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98', $1, TRUE), ('pver_2', $2, TRUE)",
            .{ profile_a, profile_b },
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO workspace_active_config (workspace_id, config_version_id) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98')",
            .{},
        );
        q.deinit();
    }

    var resolved_a = (try profile_resolver.loadWorkspaceActiveProfile(std.testing.allocator, db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11")) orelse return error.TestUnexpectedResult;
    defer resolved_a.deinit();
    try std.testing.expectEqualStrings("acme-harness-v1", resolved_a.agent_id);
    try std.testing.expectEqual(@as(usize, 3), resolved_a.stages.len);

    {
        var q = try db_ctx.conn.query(
            "UPDATE workspace_active_config SET config_version_id = 'pver_2' WHERE workspace_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11'",
            .{},
        );
        q.deinit();
    }

    var resolved_b = (try profile_resolver.loadWorkspaceActiveProfile(std.testing.allocator, db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11")) orelse return error.TestUnexpectedResult;
    defer resolved_b.deinit();
    try std.testing.expectEqualStrings("acme-harness-v2", resolved_b.agent_id);
    try std.testing.expectEqual(@as(usize, 4), resolved_b.stages.len);
    try std.testing.expectEqualStrings("security-review", resolved_b.stages[1].stage_id);
}

test "integration: default topology roles resolve through registry" {
    const alloc = std.testing.allocator;
    var profile = try topology.defaultProfile(alloc);
    defer profile.deinit();
    var registry = agents.SkillRegistry.init(alloc);
    defer registry.deinit();
    try agents.populateRegistryFromProfile(&registry, &profile);

    for (profile.stages) |stage| {
        try std.testing.expect(agents.resolveRoleWithRegistry(&registry, stage.role_id, stage.skill_id) != null);
    }
}
