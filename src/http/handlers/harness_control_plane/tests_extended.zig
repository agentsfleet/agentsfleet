const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const types = @import("types.zig");
const util = @import("util.zig");
const activate_mod = @import("activate.zig");
const compile_mod = @import("compile.zig");
const get_active_mod = @import("get_active.zig");

fn createTempLinkageTable(conn: *pg.Conn) !void {
    _ = try conn.exec(
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
}

fn createTempEntitlementTables(conn: *pg.Conn) !void {
    _ = try conn.exec(
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
    _ = try conn.exec(
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
}

fn seedFreeEntitlement(conn: *pg.Conn, workspace_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, created_at, updated_at)
        \\VALUES ($1, $2, 'FREE', 1, 3, 3, false, 0, 0)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ workspace_id, workspace_id });
}

test "integration: compileProfile rejects cross-workspace profile version selector" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profiles (
        \\  profile_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profile_versions (
        \\  profile_version_id TEXT PRIMARY KEY,
        \\  profile_id TEXT NOT NULL,
        \\  version INTEGER NOT NULL,
        \\  source_markdown TEXT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f43', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f13')",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profile_versions (profile_version_id, profile_id, version, source_markdown) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f43', 1, '{\"profile_id\":\"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f43\",\"stages\":[]}')",
        .{},
    );

    try std.testing.expectError(types.ControlPlaneError.ProfileNotFound, compile_mod.compileProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .profile_version_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93",
    }));
}

test "integration: compileProfile persists deterministic invalid outcome and linkage metadata" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profiles (
        \\  profile_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profile_versions (
        \\  profile_version_id TEXT PRIMARY KEY,
        \\  profile_id TEXT NOT NULL,
        \\  version INTEGER NOT NULL,
        \\  source_markdown TEXT NOT NULL,
        \\  compiled_profile_json TEXT,
        \\  compile_engine TEXT,
        \\  validation_report_json TEXT,
        \\  is_valid BOOLEAN NOT NULL DEFAULT false,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE profile_compile_jobs (
        \\  compile_job_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  requested_profile_id TEXT NOT NULL,
        \\  requested_version INTEGER NOT NULL,
        \\  state TEXT NOT NULL,
        \\  failure_reason TEXT,
        \\  validation_report_json TEXT NOT NULL,
        \\  created_at BIGINT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    try createTempLinkageTable(db_ctx.conn);
    try createTempEntitlementTables(db_ctx.conn);
    try seedFreeEntitlement(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");

    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11')",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profile_versions (profile_version_id, profile_id, version, source_markdown, compiled_profile_json, compile_engine, validation_report_json, is_valid, updated_at) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', 1, '# invalid source markdown', NULL, NULL, NULL, false, 0)",
        .{},
    );

    const out = try compile_mod.compileProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .profile_version_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
    });
    try std.testing.expect(!out.is_valid);
    try std.testing.expect(std.mem.indexOf(u8, out.validation_report_json, "PROFILE_PAYLOAD_NOT_FOUND") != null);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT state, failure_reason FROM profile_compile_jobs WHERE compile_job_id = $1",
            .{out.compile_job_id},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings("FAILED", try row.get([]const u8, 0));
        try std.testing.expectEqualStrings("deterministic validation failed", (try row.get(?[]const u8, 1)).?);
    }
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT metadata_json FROM profile_linkage_audit_artifacts WHERE compile_job_id = $1 AND artifact_type = 'COMPILE'",
            .{out.compile_job_id},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings("{\"is_valid\":false}", try row.get([]const u8, 0));
    }
}

test "integration: activateProfile rejects profile versions from another workspace" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profiles (
        \\  profile_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  status TEXT NOT NULL DEFAULT 'DRAFT',
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profile_versions (
        \\  profile_version_id TEXT PRIMARY KEY,
        \\  profile_id TEXT NOT NULL,
        \\  tenant_id TEXT NOT NULL,
        \\  is_valid BOOLEAN NOT NULL
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id, status, updated_at) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f42', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12', 'DRAFT', 0)",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profile_versions (profile_version_id, profile_id, tenant_id, is_valid) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f42', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', true)",
        .{},
    );
    try createTempLinkageTable(db_ctx.conn);
    try createTempEntitlementTables(db_ctx.conn);
    try seedFreeEntitlement(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");

    try std.testing.expectError(types.ControlPlaneError.ProfileNotFound, activate_mod.activateProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .profile_version_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92",
        .activated_by = "operator",
    }));
}

test "integration: compileProfile is atomic and rolls back on linkage persist failure" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profiles (
        \\  profile_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  status TEXT NOT NULL DEFAULT 'DRAFT',
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profile_versions (
        \\  profile_version_id TEXT PRIMARY KEY,
        \\  profile_id TEXT NOT NULL,
        \\  tenant_id TEXT NOT NULL,
        \\  version INTEGER NOT NULL,
        \\  source_markdown TEXT NOT NULL,
        \\  compiled_profile_json TEXT,
        \\  compile_engine TEXT,
        \\  validation_report_json TEXT,
        \\  is_valid BOOLEAN NOT NULL DEFAULT false,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE profile_compile_jobs (
        \\  compile_job_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  requested_profile_id TEXT NOT NULL,
        \\  requested_version INTEGER NOT NULL,
        \\  state TEXT NOT NULL,
        \\  failure_reason TEXT,
        \\  validation_report_json TEXT NOT NULL,
        \\  created_at BIGINT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    try createTempLinkageTable(db_ctx.conn);
    try createTempEntitlementTables(db_ctx.conn);
    try seedFreeEntitlement(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11");
    _ = try db_ctx.conn.exec(
        \\CREATE OR REPLACE FUNCTION reject_profile_linkage_insert_test()
        \\RETURNS trigger LANGUAGE plpgsql AS $$
        \\BEGIN
        \\  RAISE EXCEPTION 'forced linkage failure';
        \\END;
        \\$$
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TRIGGER trg_profile_linkage_no_insert_test
        \\BEFORE INSERT ON profile_linkage_audit_artifacts
        \\FOR EACH ROW EXECUTE FUNCTION reject_profile_linkage_insert_test()
    , .{});

    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id, status, updated_at) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', 'DRAFT', 0)",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profile_versions (profile_version_id, profile_id, tenant_id, version, source_markdown, compiled_profile_json, compile_engine, validation_report_json, is_valid, updated_at) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', 1, '# invalid source', NULL, NULL, NULL, false, 0)",
        .{},
    );

    try std.testing.expectError(error.PgError, compile_mod.compileProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .profile_version_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
    }));

    {
        var q = PgQuery.from(try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM profile_compile_jobs", .{}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT compile_engine, validation_report_json FROM agent_profile_versions WHERE profile_version_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91'",
            .{},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expect((try row.get(?[]const u8, 0)) == null);
        try std.testing.expect((try row.get(?[]const u8, 1)) == null);
    }
}

test "integration: activateProfile is atomic and rolls back on linkage persist failure" {
    const db_ctx = (try util.openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profiles (
        \\  profile_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  status TEXT NOT NULL DEFAULT 'DRAFT',
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_profile_versions (
        \\  profile_version_id TEXT PRIMARY KEY,
        \\  profile_id TEXT NOT NULL,
        \\  tenant_id TEXT NOT NULL,
        \\  is_valid BOOLEAN NOT NULL
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspace_active_profile (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  profile_version_id TEXT NOT NULL,
        \\  activated_by TEXT NOT NULL,
        \\  activated_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    try createTempLinkageTable(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\CREATE OR REPLACE FUNCTION reject_profile_linkage_insert_test()
        \\RETURNS trigger LANGUAGE plpgsql AS $$
        \\BEGIN
        \\  RAISE EXCEPTION 'forced linkage failure';
        \\END;
        \\$$
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TRIGGER trg_profile_linkage_no_insert_test
        \\BEFORE INSERT ON profile_linkage_audit_artifacts
        \\FOR EACH ROW EXECUTE FUNCTION reject_profile_linkage_insert_test()
    , .{});

    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id, status, updated_at) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', 'DRAFT', 0)",
        .{},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO agent_profile_versions (profile_version_id, profile_id, tenant_id, is_valid) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01', true)",
        .{},
    );

    try std.testing.expectError(error.PgError, activate_mod.activateProfile(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", .{
        .profile_version_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f91",
        .activated_by = "operator",
    }));

    {
        var q = PgQuery.from(try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM workspace_active_profile", .{}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 0), try row.get(i64, 0));
    }
    {
        var q = PgQuery.from(try db_ctx.conn.query("SELECT status FROM agent_profiles WHERE profile_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f41'", .{}));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings("DRAFT", try row.get([]const u8, 0));
    }
}
