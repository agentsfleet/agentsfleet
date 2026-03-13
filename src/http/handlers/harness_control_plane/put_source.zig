const std = @import("std");
const pg = @import("pg");
const prompt_events = @import("../../../observability/prompt_events.zig");
const types = @import("types.zig");
const util = @import("util.zig");

pub fn putSource(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: types.PutSourceInput,
) (types.ControlPlaneError || anyerror)!types.PutSourceOutput {
    if (input.source_markdown.len == 0) return types.ControlPlaneError.InvalidRequest;

    var ws = try conn.query("SELECT tenant_id FROM workspaces WHERE workspace_id = $1", .{workspace_id});
    defer ws.deinit();
    const ws_row = (try ws.next()) orelse return types.ControlPlaneError.WorkspaceNotFound;
    const tenant_id = try ws_row.get([]const u8, 0);

    const profile_id = try util.normalizeProfileId(alloc, input.profile_id);
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
    const profile_version_id = try util.generateProfileVersionId(alloc);
    if (!util.isSupportedProfileVersionId(profile_version_id)) return types.ControlPlaneError.InvalidIdShape;

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
