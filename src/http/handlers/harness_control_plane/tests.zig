const std = @import("std");
const pg = @import("pg");
const types = @import("types.zig");
const util = @import("util.zig");
const activate_mod = @import("activate.zig");
const compile_mod = @import("compile.zig");
const get_active_mod = @import("get_active.zig");

fn createTempLinkageTable(conn: *pg.Conn) !void {
    var q = try conn.query(
        \\CREATE TEMP TABLE profile_linkage_audit_artifacts (
        \\  artifact_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  artifact_type TEXT NOT NULL,
        \\  profile_version_id TEXT NOT NULL,
        \\  compile_job_id TEXT,
        \\  run_id TEXT,
        \\  parent_artifact_id TEXT,
        \\  metadata_json TEXT NOT NULL DEFAULT '{}',
        \\  created_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    q.deinit();
}

fn createTempEntitlementTables(conn: *pg.Conn) !void {
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_entitlements (
            \\  entitlement_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE,
            \\  plan_tier TEXT NOT NULL,
            \\  max_profiles INTEGER NOT NULL,
            \\  max_stages INTEGER NOT NULL,
            \\  max_distinct_skills INTEGER NOT NULL,
            \\  allow_custom_skills BOOLEAN NOT NULL,
            \\  enable_agent_scoring BOOLEAN NOT NULL DEFAULT FALSE,
            \\  agent_scoring_weights_json TEXT NOT NULL DEFAULT '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE entitlement_policy_audit_snapshots (
            \\  snapshot_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL,
            \\  boundary TEXT NOT NULL,
            \\  decision TEXT NOT NULL,
            \\  reason_code TEXT NOT NULL,
            \\  plan_tier TEXT NOT NULL,
            \\  policy_json TEXT NOT NULL,
            \\  observed_json TEXT NOT NULL,
            \\  actor TEXT NOT NULL,
            \\  created_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
}

fn seedFreeEntitlement(conn: *pg.Conn, workspace_id: []const u8) !void {
    var q = try conn.query(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, created_at, updated_at)
        \\VALUES ($1, $2, 'FREE', 1, 3, 3, false, 0, 0)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ workspace_id, workspace_id });
    q.deinit();
}

test "integration: activateProfile rejects invalid profile versions" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_profiles (
            \\  profile_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT NOT NULL,
            \\  workspace_id TEXT NOT NULL,
            \\  status TEXT NOT NULL DEFAULT 'DRAFT',
            \\  updated_at BIGINT NOT NULL DEFAULT 0
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_profile_versions (
            \\  profile_version_id TEXT PRIMARY KEY,
            \\  profile_id TEXT NOT NULL,
            \\  tenant_id TEXT NOT NULL,
            \\  is_valid BOOLEAN NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    try createTempLinkageTable(db_ctx.conn);
    try createTempEntitlementTables(db_ctx.conn);
    try seedFreeEntitlement(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");

    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id, status, updated_at) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', 'DRAFT', 0)",
            .{},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_profile_versions (profile_version_id, profile_id, tenant_id, is_valid) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', false)",
            .{},
        );
        q.deinit();
    }

    try std.testing.expectError(types.ControlPlaneError.ProfileInvalid, activate_mod.activateProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .profile_version_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        .activated_by = "test",
    }));
}

test "integration: getActiveProfile falls back to default-v1 when no active profile exists" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspace_active_profile (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT NOT NULL,
            \\  profile_version_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_profile_versions (
            \\  profile_version_id TEXT PRIMARY KEY,
            \\  compiled_profile_json TEXT
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    try createTempLinkageTable(db_ctx.conn);
    try createTempEntitlementTables(db_ctx.conn);
    try seedFreeEntitlement(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");

    const out = try get_active_mod.getActiveProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f19");
    defer std.testing.allocator.free(out.profile_json);
    try std.testing.expectEqualStrings("default-v1", out.source);
    try std.testing.expect(out.profile_id == null);
    try std.testing.expect(out.profile_version_id == null);
    try std.testing.expect(out.run_snapshot_version == null);
    try std.testing.expect(out.active_at == null);
}

test "integration: activate/getActive profile identity contract includes snapshot linkage fields" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_profiles (
            \\  profile_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT NOT NULL,
            \\  workspace_id TEXT NOT NULL,
            \\  status TEXT NOT NULL DEFAULT 'DRAFT',
            \\  updated_at BIGINT NOT NULL DEFAULT 0
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE agent_profile_versions (
            \\  profile_version_id TEXT PRIMARY KEY,
            \\  profile_id TEXT NOT NULL,
            \\  tenant_id TEXT NOT NULL,
            \\  is_valid BOOLEAN NOT NULL,
            \\  compiled_profile_json TEXT
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspace_active_profile (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT NOT NULL,
            \\  profile_version_id TEXT NOT NULL,
            \\  activated_by TEXT NOT NULL,
            \\  activated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    try createTempLinkageTable(db_ctx.conn);
    try createTempEntitlementTables(db_ctx.conn);
    try seedFreeEntitlement(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");

    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id, status, updated_at) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', 'DRAFT', 0)",
            .{},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_profile_versions (profile_version_id, profile_id, tenant_id, is_valid, compiled_profile_json) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', TRUE, '{\"profile_id\":\"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41\",\"stages\":[]}')",
            .{},
        );
        q.deinit();
    }

    const activated = try activate_mod.activateProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .profile_version_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        .activated_by = "operator",
    });
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41", activated.profile_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91", activated.profile_version_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91", activated.run_snapshot_version);

    const active = try get_active_mod.getActiveProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    defer std.testing.allocator.free(active.profile_json);
    try std.testing.expectEqualStrings("active", active.source);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41", active.profile_id.?);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91", active.profile_version_id.?);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91", active.run_snapshot_version.?);
    try std.testing.expect(active.active_at != null);
}

comptime {
    _ = @import("tests_extended.zig");
}
