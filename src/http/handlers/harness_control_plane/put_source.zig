const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const prompt_events = @import("../../../observability/prompt_events.zig");
const types = @import("types.zig");
const util = @import("util.zig");

const DEFAULT_AGENT_NAME = "Workspace Harness";
const STATUS_DRAFT = "DRAFT";
const COMPILE_ENGINE_DETERMINISTIC_V1 = "deterministic-v1";
const VALIDATION_STATUS_PENDING_JSON = "{\"status\":\"pending\"}";

pub fn putSource(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: types.PutSourceInput,
) (types.ControlPlaneError || anyerror)!types.PutSourceOutput {
    if (input.source_markdown.len == 0) return types.ControlPlaneError.InvalidRequest;

    var ws = PgQuery.from(try conn.query("SELECT tenant_id FROM workspaces WHERE workspace_id = $1", .{workspace_id}));
    defer ws.deinit();
    const ws_row = (try ws.next()) orelse return types.ControlPlaneError.WorkspaceNotFound;
    const tenant_id = try alloc.dupe(u8, try ws_row.get([]const u8, 0));

    const agent_id = try util.normalizeAgentId(alloc, input.agent_id);
    errdefer alloc.free(agent_id);
    const agent_name = input.name orelse DEFAULT_AGENT_NAME;
    const now_ms = std.time.milliTimestamp();

    _ = try conn.exec(
        \\INSERT INTO agent_profiles (agent_id, tenant_id, workspace_id, name, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $6)
        \\ON CONFLICT (agent_id) DO UPDATE
        \\SET name = EXCLUDED.name,
        \\    updated_at = EXCLUDED.updated_at
    , .{ agent_id, tenant_id, workspace_id, agent_name, STATUS_DRAFT, now_ms });

    var vq = PgQuery.from(try conn.query(
        "SELECT COALESCE(MAX(version), 0)::INTEGER FROM agent_config_versions WHERE agent_id = $1",
        .{agent_id},
    ));
    defer vq.deinit();
    const next_version: i32 = blk: {
        const row = (try vq.next()) orelse break :blk @as(i32, 1);
        break :blk (try row.get(i32, 0)) + 1;
    };
    const config_version_id = try util.generateConfigVersionId(alloc);
    if (!util.isSupportedConfigVersionId(config_version_id)) return types.ControlPlaneError.InvalidIdShape;

    _ = try conn.exec(
        \\INSERT INTO agent_config_versions
        \\  (config_version_id, tenant_id, agent_id, version, source_markdown, compiled_profile_json, compile_engine, validation_report_json, is_valid, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, false, $8, $8)
    , .{ config_version_id, tenant_id, agent_id, next_version, input.source_markdown, COMPILE_ENGINE_DETERMINISTIC_V1, VALIDATION_STATUS_PENDING_JSON, now_ms });

    const birth_meta = std.fmt.allocPrint(alloc, "{{\"version\":{d}}}", .{next_version}) catch "{}";
    prompt_events.emitBestEffort(conn, .{
        .event_type = .prompt_birth,
        .workspace_id = workspace_id,
        .tenant_id = tenant_id,
        .agent_id = agent_id,
        .config_version_id = config_version_id,
        .metadata_json = birth_meta,
        .ts_ms = now_ms,
    });

    return .{
        .agent_id = agent_id,
        .config_version_id = config_version_id,
        .version = next_version,
    };
}
