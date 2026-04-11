const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const log = std.log.scoped(.audit);

pub const RunLinkage = struct {
    run_artifact_id: ?[]u8 = null,
    activate_artifact_id: ?[]u8 = null,
    compile_artifact_id: ?[]u8 = null,
    config_version_id: ?[]u8 = null,
    compile_job_id: ?[]u8 = null,
};

pub fn insertCompileArtifact(
    conn: *pg.Conn,
    tenant_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
    compile_job_id: []const u8,
    is_valid: bool,
    created_at: i64,
) !void {
    var artifact_id_buf: [36]u8 = undefined;
    const artifact_id = generateUuidV7(&artifact_id_buf);
    const meta = if (is_valid) "{\"is_valid\":true}" else "{\"is_valid\":false}";

    _ = try conn.exec(
        \\INSERT INTO config_linkage_audit_artifacts
        \\  (artifact_id, tenant_id, workspace_id, artifact_type, config_version_id, compile_job_id, run_id, parent_artifact_id, metadata_json, created_at)
        \\VALUES ($1, $2, $3, 'COMPILE', $4, $5, NULL, NULL, $6, $7)
    , .{ artifact_id, tenant_id, workspace_id, config_version_id, compile_job_id, meta, created_at });
    log.info("linkage recorded type=COMPILE workspace_id={s} config_version_id={s}", .{ workspace_id, config_version_id });
}

pub fn insertActivateArtifact(
    conn: *pg.Conn,
    tenant_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
    activated_by: []const u8,
    created_at: i64,
) !void {
    var artifact_id_buf: [36]u8 = undefined;
    const artifact_id = generateUuidV7(&artifact_id_buf);

    var compile_q = PgQuery.from(try conn.query(
        \\SELECT artifact_id, compile_job_id
        \\FROM config_linkage_audit_artifacts
        \\WHERE workspace_id = $1 AND config_version_id = $2 AND artifact_type = 'COMPILE'
        \\ORDER BY created_at DESC
        \\LIMIT 1
    , .{ workspace_id, config_version_id }));
    defer compile_q.deinit();

    var parent_artifact_id: ?[]const u8 = null;
    var compile_job_id: ?[]const u8 = null;
    if (try compile_q.next()) |row| {
        parent_artifact_id = try row.get([]const u8, 0);
        compile_job_id = try row.get(?[]const u8, 1);
    }

    _ = try conn.exec(
        \\INSERT INTO config_linkage_audit_artifacts
        \\  (artifact_id, tenant_id, workspace_id, artifact_type, config_version_id, compile_job_id, run_id, parent_artifact_id, metadata_json, created_at)
        \\VALUES ($1, $2, $3, 'ACTIVATE', $4, $5, NULL, $6, json_build_object('activated_by', $7)::text, $8)
    , .{ artifact_id, tenant_id, workspace_id, config_version_id, compile_job_id, parent_artifact_id, activated_by, created_at });
    log.info("linkage recorded type=ACTIVATE workspace_id={s} config_version_id={s}", .{ workspace_id, config_version_id });
}

pub fn insertRunArtifact(
    conn: *pg.Conn,
    tenant_id: []const u8,
    workspace_id: []const u8,
    run_id: []const u8,
    config_version_id: []const u8,
    created_at: i64,
) !void {
    var artifact_id_buf: [36]u8 = undefined;
    const artifact_id = generateUuidV7(&artifact_id_buf);

    var act_q = PgQuery.from(try conn.query(
        \\SELECT artifact_id, compile_job_id
        \\FROM config_linkage_audit_artifacts
        \\WHERE workspace_id = $1 AND config_version_id = $2 AND artifact_type = 'ACTIVATE' AND created_at <= $3
        \\ORDER BY created_at DESC
        \\LIMIT 1
    , .{ workspace_id, config_version_id, created_at }));
    defer act_q.deinit();

    var parent_artifact_id: ?[]const u8 = null;
    var compile_job_id: ?[]const u8 = null;
    if (try act_q.next()) |row| {
        parent_artifact_id = try row.get([]const u8, 0);
        compile_job_id = try row.get(?[]const u8, 1);
    }

    _ = try conn.exec(
        \\INSERT INTO config_linkage_audit_artifacts
        \\  (artifact_id, tenant_id, workspace_id, artifact_type, config_version_id, compile_job_id, run_id, parent_artifact_id, metadata_json, created_at)
        \\VALUES ($1, $2, $3, 'RUN', $4, $5, $6, $7, '{}', $8)
    , .{ artifact_id, tenant_id, workspace_id, config_version_id, compile_job_id, run_id, parent_artifact_id, created_at });
    log.info("linkage recorded type=RUN workspace_id={s} run_id={s}", .{ workspace_id, run_id });
}

pub fn fetchRunLinkage(conn: *pg.Conn, alloc: std.mem.Allocator, run_id: []const u8) !?RunLinkage {
    var q = PgQuery.from(try conn.query(
        \\SELECT run_art.artifact_id,
        \\       run_art.config_version_id,
        \\       run_art.compile_job_id,
        \\       run_art.parent_artifact_id,
        \\       act.parent_artifact_id
        \\FROM config_linkage_audit_artifacts run_art
        \\LEFT JOIN config_linkage_audit_artifacts act
        \\  ON act.artifact_id = run_art.parent_artifact_id AND act.artifact_type = 'ACTIVATE'
        \\WHERE run_art.run_id = $1 AND run_art.artifact_type = 'RUN'
        \\ORDER BY run_art.created_at DESC
        \\LIMIT 1
    , .{run_id}));
    defer q.deinit();

    const row = (try q.next()) orelse return null;

    const run_artifact_id = if (try row.get(?[]const u8, 0)) |v| try alloc.dupe(u8, v) else null;
    errdefer if (run_artifact_id) |v| alloc.free(v);
    const config_version_id = if (try row.get(?[]const u8, 1)) |v| try alloc.dupe(u8, v) else null;
    errdefer if (config_version_id) |v| alloc.free(v);
    const compile_job_id = if (try row.get(?[]const u8, 2)) |v| try alloc.dupe(u8, v) else null;
    errdefer if (compile_job_id) |v| alloc.free(v);
    const activate_artifact_id = if (try row.get(?[]const u8, 3)) |v| try alloc.dupe(u8, v) else null;
    errdefer if (activate_artifact_id) |v| alloc.free(v);
    const compile_artifact_id = if (try row.get(?[]const u8, 4)) |v| try alloc.dupe(u8, v) else null;

    return .{
        .run_artifact_id = run_artifact_id,
        .config_version_id = config_version_id,
        .compile_job_id = compile_job_id,
        .activate_artifact_id = activate_artifact_id,
        .compile_artifact_id = compile_artifact_id,
    };
}

pub fn freeRunLinkage(alloc: std.mem.Allocator, linkage: *RunLinkage) void {
    if (linkage.run_artifact_id) |v| alloc.free(v);
    if (linkage.activate_artifact_id) |v| alloc.free(v);
    if (linkage.compile_artifact_id) |v| alloc.free(v);
    if (linkage.config_version_id) |v| alloc.free(v);
    if (linkage.compile_job_id) |v| alloc.free(v);
    linkage.* = .{};
}

fn generateUuidV7(buf: []u8) []const u8 {
    // check-pg-drain: ok — no conn.query() in this function; checker misattributes test-block queries
    var raw: [16]u8 = undefined;
    std.crypto.random.bytes(&raw);

    const ts_ms: u64 = @intCast(std.time.milliTimestamp());
    raw[0] = @intCast((ts_ms >> 40) & 0xff);
    raw[1] = @intCast((ts_ms >> 32) & 0xff);
    raw[2] = @intCast((ts_ms >> 24) & 0xff);
    raw[3] = @intCast((ts_ms >> 16) & 0xff);
    raw[4] = @intCast((ts_ms >> 8) & 0xff);
    raw[5] = @intCast(ts_ms & 0xff);
    raw[6] = (raw[6] & 0x0f) | 0x70;
    raw[8] = (raw[8] & 0x3f) | 0x80;

    const hex = std.fmt.bytesToHex(raw, .lower);
    return std.fmt.bufPrint(
        buf,
        "{s}-{s}-{s}-{s}-{s}",
        .{ hex[0..8], hex[8..12], hex[12..16], hex[16..20], hex[20..32] },
    ) catch "00000000-0000-7000-8000-000000000000";
}

test "integration: linkage chain is queryable for run" {
    const common = @import("../http/handlers/common.zig");

    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE config_linkage_audit_artifacts (
        \\  artifact_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  artifact_type TEXT NOT NULL,
        \\  config_version_id TEXT NOT NULL,
        \\  compile_job_id TEXT,
        \\  run_id TEXT,
        \\  parent_artifact_id TEXT,
        \\  metadata_json TEXT NOT NULL DEFAULT '{}',
        \\  created_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});

    try insertCompileArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", "0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97", true, 10);
    try insertActivateArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", "operator", 20);
    try insertRunArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", 30);

    var linkage = (try fetchRunLinkage(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99")).?;
    defer freeRunLinkage(std.testing.allocator, &linkage);

    try std.testing.expect(linkage.run_artifact_id != null);
    try std.testing.expect(linkage.activate_artifact_id != null);
    try std.testing.expect(linkage.compile_artifact_id != null);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", linkage.config_version_id.?);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97", linkage.compile_job_id.?);
}

test "integration: linkage artifacts are immutable and reject updates" {
    const common = @import("../http/handlers/common.zig");

    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE config_linkage_audit_artifacts (
        \\  artifact_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  artifact_type TEXT NOT NULL,
        \\  config_version_id TEXT NOT NULL,
        \\  compile_job_id TEXT,
        \\  run_id TEXT,
        \\  parent_artifact_id TEXT,
        \\  metadata_json TEXT NOT NULL DEFAULT '{}',
        \\  created_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});

    _ = try db_ctx.conn.exec(
        \\CREATE OR REPLACE FUNCTION reject_profile_linkage_mutation_test()
        \\RETURNS trigger LANGUAGE plpgsql AS $$
        \\BEGIN
        \\    RAISE EXCEPTION 'config_linkage_audit_artifacts is append-only';
        \\END;
        \\$$
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TRIGGER trg_profile_linkage_no_update_test
        \\BEFORE UPDATE ON config_linkage_audit_artifacts
        \\FOR EACH ROW EXECUTE FUNCTION reject_profile_linkage_mutation_test()
    , .{});

    try insertCompileArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", "0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97", true, 10);

    try std.testing.expectError(error.PgError, db_ctx.conn.query(
        "UPDATE config_linkage_audit_artifacts SET metadata_json = '{\"x\":1}' WHERE artifact_type = 'COMPILE'",
        .{},
    ));
}

test "integration: activate linkage metadata preserves escaped activated_by value" {
    const common = @import("../http/handlers/common.zig");

    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE config_linkage_audit_artifacts (
        \\  artifact_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  artifact_type TEXT NOT NULL,
        \\  config_version_id TEXT NOT NULL,
        \\  compile_job_id TEXT,
        \\  run_id TEXT,
        \\  parent_artifact_id TEXT,
        \\  metadata_json TEXT NOT NULL DEFAULT '{}',
        \\  created_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});

    try insertCompileArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", "0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97", true, 10);
    const activated_by = "operator \"alpha\" \\ slash";
    try insertActivateArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", activated_by, 20);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT metadata_json FROM config_linkage_audit_artifacts WHERE artifact_type = 'ACTIVATE' LIMIT 1",
        .{},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const metadata_json = try row.get([]const u8, 0);
    const parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, metadata_json, .{});
    defer parsed.deinit();
    const meta_obj = parsed.value.object;
    const value = meta_obj.get("activated_by") orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(std.json.Value.Tag.string, value);
    try std.testing.expectEqualStrings(activated_by, value.string);
}

test "integration: run linkage insert fails closed when snapshot profile version does not exist" {
    const common = @import("../http/handlers/common.zig");

    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE agent_config_versions (
        \\  config_version_id TEXT PRIMARY KEY
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE runs (
        \\  run_id TEXT PRIMARY KEY
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE config_linkage_audit_artifacts (
        \\  artifact_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  artifact_type TEXT NOT NULL,
        \\  config_version_id TEXT NOT NULL REFERENCES agent_config_versions(config_version_id),
        \\  compile_job_id TEXT,
        \\  run_id TEXT REFERENCES runs(run_id),
        \\  parent_artifact_id TEXT,
        \\  metadata_json TEXT NOT NULL DEFAULT '{}',
        \\  created_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    , .{});
    _ = try db_ctx.conn.exec("INSERT INTO runs (run_id) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99')", .{});

    try std.testing.expectError(error.PgError, insertRunArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6fff", 30));
}
