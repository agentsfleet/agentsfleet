const std = @import("std");
const pg = @import("pg");
const db = @import("../../db/pool.zig");
const topology = @import("../../pipeline/topology.zig");
const harness = @import("../../harness/control_plane.zig");
const prompt_events = @import("../../observability/prompt_events.zig");

pub const PutSourceInput = struct {
    profile_id: ?[]const u8 = null,
    name: ?[]const u8 = null,
    source_markdown: []const u8,
};

pub const PutSourceOutput = struct {
    profile_id: []u8,
    profile_version_id: []const u8,
    version: i32,
};

pub const CompileInput = struct {
    profile_id: ?[]const u8 = null,
    profile_version_id: ?[]const u8 = null,
};

pub const CompileOutput = struct {
    compile_job_id: []const u8,
    profile_id: []const u8,
    profile_version_id: []const u8,
    is_valid: bool,
    validation_report_json: []const u8,
};

pub const ActivateInput = struct {
    profile_version_id: []const u8,
    activated_by: ?[]const u8 = null,
};

pub const ActivateOutput = struct {
    profile_version_id: []const u8,
    activated_by: []const u8,
    activated_at: i64,
};

pub const ActiveOutput = struct {
    source: []const u8,
    profile_version_id: ?[]const u8,
    profile_json: []u8,
};

pub const ControlPlaneError = error{
    InvalidRequest,
    WorkspaceNotFound,
    ProfileNotFound,
    ProfileInvalid,
    CompileFailed,
};

pub fn putSource(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: PutSourceInput,
) (ControlPlaneError || anyerror)!PutSourceOutput {
    if (input.source_markdown.len == 0) return ControlPlaneError.InvalidRequest;

    var ws = try conn.query("SELECT tenant_id FROM workspaces WHERE workspace_id = $1", .{workspace_id});
    defer ws.deinit();
    const ws_row = (try ws.next()) orelse return ControlPlaneError.WorkspaceNotFound;
    const tenant_id = try ws_row.get([]const u8, 0);

    const profile_id = try normalizeProfileId(alloc, workspace_id, input.profile_id);
    errdefer alloc.free(profile_id);
    const profile_name = input.name orelse "Workspace Harness";
    const now_ms = std.time.milliTimestamp();

    var upsert_profile = try conn.query(
        \\INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id, name, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, 'DRAFT', $5, $5)
        \\ON CONFLICT (profile_id) DO UPDATE
        \\SET name = EXCLUDED.name,
        \\    updated_at = EXCLUDED.updated_at
    , .{ profile_id, tenant_id, workspace_id, profile_name, now_ms });
    upsert_profile.deinit();

    var vq = try conn.query(
        "SELECT COALESCE(MAX(version), 0)::INTEGER FROM agent_profile_versions WHERE profile_id = $1",
        .{profile_id},
    );
    defer vq.deinit();
    const next_version: i32 = if (try vq.next()) |row| (try row.get(i32, 0)) + 1 else 1;
    const profile_version_id = prefixedId(alloc, "pver");

    var insert_version = try conn.query(
        \\INSERT INTO agent_profile_versions
        \\  (profile_version_id, tenant_id, profile_id, version, source_markdown, compiled_profile_json, compile_engine, validation_report_json, is_valid, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, NULL, 'deterministic-v1', '{"status":"pending"}', false, $6, $6)
    , .{ profile_version_id, tenant_id, profile_id, next_version, input.source_markdown, now_ms });
    insert_version.deinit();

    const birth_meta = std.fmt.allocPrint(alloc, "{{\"version\":{d}}}", .{next_version}) catch "{}";
    prompt_events.emitBestEffort(conn, .{
        .event_type = .prompt_birth,
        .workspace_id = workspace_id,
        .tenant_id = tenant_id,
        .profile_id = profile_id,
        .profile_version_id = profile_version_id,
        .metadata_json = birth_meta,
        .ts_ms = now_ms,
    });

    return .{
        .profile_id = profile_id,
        .profile_version_id = profile_version_id,
        .version = next_version,
    };
}

pub fn compileProfile(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: CompileInput,
) (ControlPlaneError || anyerror)!CompileOutput {
    const selection_sql = if (input.profile_version_id != null)
        \\SELECT v.profile_version_id, v.profile_id, v.version, v.source_markdown, p.tenant_id
        \\FROM agent_profile_versions v
        \\JOIN agent_profiles p ON p.profile_id = v.profile_id
        \\WHERE p.workspace_id = $1 AND v.profile_version_id = $2
        \\LIMIT 1
    else if (input.profile_id != null)
        \\SELECT v.profile_version_id, v.profile_id, v.version, v.source_markdown, p.tenant_id
        \\FROM agent_profile_versions v
        \\JOIN agent_profiles p ON p.profile_id = v.profile_id
        \\WHERE p.workspace_id = $1 AND v.profile_id = $2
        \\ORDER BY v.version DESC
        \\LIMIT 1
    else
        \\SELECT v.profile_version_id, v.profile_id, v.version, v.source_markdown, p.tenant_id
        \\FROM agent_profile_versions v
        \\JOIN agent_profiles p ON p.profile_id = v.profile_id
        \\WHERE p.workspace_id = $1
        \\ORDER BY v.created_at DESC
        \\LIMIT 1
    ;

    const selector_arg = input.profile_version_id orelse input.profile_id orelse "";
    var pick = if (input.profile_version_id == null and input.profile_id == null)
        try conn.query(selection_sql, .{workspace_id})
    else
        try conn.query(selection_sql, .{ workspace_id, selector_arg });
    defer pick.deinit();

    const row = (try pick.next()) orelse return ControlPlaneError.ProfileNotFound;
    const profile_version_id = try row.get([]const u8, 0);
    const profile_id = try row.get([]const u8, 1);
    const version = try row.get(i32, 2);
    const source_markdown = try row.get([]const u8, 3);
    const tenant_id = try row.get([]const u8, 4);

    const compile_job_id = prefixedId(alloc, "cjob");
    const now_ms = std.time.milliTimestamp();
    var insert_job = try conn.query(
        \\INSERT INTO profile_compile_jobs
        \\  (compile_job_id, tenant_id, workspace_id, requested_profile_id, requested_version, state, failure_reason, validation_report_json, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, 'RUNNING', NULL, '{"status":"running"}', $6, $6)
    , .{ compile_job_id, tenant_id, workspace_id, profile_id, version, now_ms });
    insert_job.deinit();

    var outcome = harness.compileHarnessMarkdown(alloc, source_markdown) catch return ControlPlaneError.CompileFailed;
    defer outcome.deinit(alloc);

    const finish_ts = std.time.milliTimestamp();
    var update_profile = try conn.query(
        \\UPDATE agent_profile_versions
        \\SET compiled_profile_json = $1,
        \\    compile_engine = 'deterministic-v1',
        \\    validation_report_json = $2,
        \\    is_valid = $3,
        \\    updated_at = $4
        \\WHERE profile_version_id = $5
    , .{
        outcome.compiled_profile_json,
        outcome.validation_report_json,
        outcome.is_valid,
        finish_ts,
        profile_version_id,
    });
    update_profile.deinit();

    var update_job = try conn.query(
        \\UPDATE profile_compile_jobs
        \\SET state = $1,
        \\    failure_reason = $2,
        \\    validation_report_json = $3,
        \\    updated_at = $4
        \\WHERE compile_job_id = $5
    , .{
        if (outcome.is_valid) "SUCCEEDED" else "FAILED",
        if (outcome.is_valid) null else "deterministic validation failed",
        outcome.validation_report_json,
        finish_ts,
        compile_job_id,
    });
    update_job.deinit();

    if (outcome.is_valid) {
        const accepted_meta = std.fmt.allocPrint(alloc, "{{\"compile_job_id\":\"{s}\",\"version\":{d}}}", .{ compile_job_id, version }) catch "{}";
        prompt_events.emitBestEffort(conn, .{
            .event_type = .prompt_accepted,
            .workspace_id = workspace_id,
            .tenant_id = tenant_id,
            .profile_id = profile_id,
            .profile_version_id = profile_version_id,
            .metadata_json = accepted_meta,
            .ts_ms = finish_ts,
        });
    }

    return .{
        .compile_job_id = compile_job_id,
        .profile_id = profile_id,
        .profile_version_id = profile_version_id,
        .is_valid = outcome.is_valid,
        .validation_report_json = outcome.validation_report_json,
    };
}

pub fn activateProfile(
    conn: *pg.Conn,
    workspace_id: []const u8,
    input: ActivateInput,
) (ControlPlaneError || anyerror)!ActivateOutput {
    var q = try conn.query(
        \\SELECT v.profile_id, v.is_valid, p.tenant_id
        \\FROM agent_profile_versions v
        \\JOIN agent_profiles p ON p.profile_id = v.profile_id
        \\WHERE p.workspace_id = $1 AND v.profile_version_id = $2
        \\LIMIT 1
    , .{ workspace_id, input.profile_version_id });
    defer q.deinit();

    const row = (try q.next()) orelse return ControlPlaneError.ProfileNotFound;
    const profile_id = try row.get([]const u8, 0);
    const is_valid = try row.get(bool, 1);
    const tenant_id = try row.get([]const u8, 2);
    if (!is_valid) return ControlPlaneError.ProfileInvalid;

    const now_ms = std.time.milliTimestamp();
    const activated_by = input.activated_by orelse "api";
    var upsert = try conn.query(
        \\INSERT INTO workspace_active_profile (workspace_id, tenant_id, profile_version_id, activated_by, activated_at)
        \\VALUES ($1, $2, $3, $4, $5)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id,
        \\    profile_version_id = EXCLUDED.profile_version_id,
        \\    activated_by = EXCLUDED.activated_by,
        \\    activated_at = EXCLUDED.activated_at
    , .{ workspace_id, tenant_id, input.profile_version_id, activated_by, now_ms });
    upsert.deinit();

    var mark_active = try conn.query(
        "UPDATE agent_profiles SET status = CASE WHEN profile_id = $1 THEN 'ACTIVE' ELSE status END, updated_at = $2 WHERE workspace_id = $3",
        .{ profile_id, now_ms, workspace_id },
    );
    mark_active.deinit();

    prompt_events.emitBestEffort(conn, .{
        .event_type = .prompt_applied,
        .workspace_id = workspace_id,
        .tenant_id = tenant_id,
        .profile_id = profile_id,
        .profile_version_id = input.profile_version_id,
        .metadata_json = "{}",
        .ts_ms = now_ms,
    });

    return .{
        .profile_version_id = input.profile_version_id,
        .activated_by = activated_by,
        .activated_at = now_ms,
    };
}

pub fn getActiveProfile(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) (ControlPlaneError || anyerror)!ActiveOutput {
    var q = try conn.query(
        \\SELECT wap.profile_version_id, v.compiled_profile_json
        \\FROM workspace_active_profile wap
        \\JOIN agent_profile_versions v ON v.profile_version_id = wap.profile_version_id
        \\WHERE wap.workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();

    if (try q.next()) |row| {
        const profile_version_id = try row.get([]const u8, 0);
        const compiled_json_opt = try row.get(?[]const u8, 1);
        if (compiled_json_opt) |compiled_json| {
            return .{
                .source = "active",
                .profile_version_id = profile_version_id,
                .profile_json = try alloc.dupe(u8, compiled_json),
            };
        }
    }

    var fallback = try topology.defaultProfile(alloc);
    defer fallback.deinit();
    const fallback_json = try harness.stringifyTopologyProfile(alloc, &fallback);
    return .{
        .source = "default-v1",
        .profile_version_id = null,
        .profile_json = fallback_json,
    };
}

fn prefixedId(alloc: std.mem.Allocator, prefix: []const u8) []const u8 {
    var id: [16]u8 = undefined;
    std.crypto.random.bytes(&id);
    const hex = std.fmt.bytesToHex(id, .lower);
    return std.fmt.allocPrint(alloc, "{s}_{s}", .{ prefix, hex[0..12] }) catch "id_unknown";
}

fn normalizeProfileId(
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    provided: ?[]const u8,
) ![]u8 {
    if (provided) |raw| {
        if (raw.len == 0) return error.InvalidProfileId;
        var out: std.ArrayList(u8) = .{};
        defer out.deinit(alloc);
        for (raw) |c| {
            if (std.ascii.isAlphanumeric(c) or c == '-' or c == '_') {
                try out.append(alloc, std.ascii.toLower(c));
            } else {
                try out.append(alloc, '-');
            }
        }
        return out.toOwnedSlice(alloc);
    }
    return std.fmt.allocPrint(alloc, "{s}-harness", .{workspace_id});
}

fn openHarnessHandlerTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
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

test "integration: activateProfile rejects invalid profile versions" {
    const db_ctx = (try openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
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

    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_profiles (profile_id, tenant_id, workspace_id, status, updated_at) VALUES ('prof_1', 'tenant_1', 'ws_1', 'DRAFT', 0)",
            .{},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO agent_profile_versions (profile_version_id, profile_id, tenant_id, is_valid) VALUES ('pver_1', 'prof_1', 'tenant_1', false)",
            .{},
        );
        q.deinit();
    }

    try std.testing.expectError(ControlPlaneError.ProfileInvalid, activateProfile(db_ctx.conn, "ws_1", .{
        .profile_version_id = "pver_1",
        .activated_by = "test",
    }));
}

test "integration: getActiveProfile falls back to default-v1 when no active profile exists" {
    const db_ctx = (try openHarnessHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
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

    const out = try getActiveProfile(db_ctx.conn, std.testing.allocator, "ws_missing");
    defer std.testing.allocator.free(out.profile_json);
    try std.testing.expectEqualStrings("default-v1", out.source);
    try std.testing.expect(out.profile_version_id == null);
}
