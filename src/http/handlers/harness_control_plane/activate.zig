const std = @import("std");
const pg = @import("pg");
const prompt_events = @import("../../../observability/prompt_events.zig");
const profile_linkage = @import("../../../audit/profile_linkage.zig");
const types = @import("types.zig");
const util = @import("util.zig");

fn beginTx(conn: *pg.Conn) !void {
    var tx = try conn.query("BEGIN", .{});
    tx.deinit();
}

fn commitTx(conn: *pg.Conn) !void {
    var tx = try conn.query("COMMIT", .{});
    tx.deinit();
}

fn rollbackTx(conn: *pg.Conn) void {
    var tx = conn.query("ROLLBACK", .{}) catch return;
    tx.deinit();
}

pub fn activateProfile(
    conn: *pg.Conn,
    workspace_id: []const u8,
    input: types.ActivateInput,
) (types.ControlPlaneError || anyerror)!types.ActivateOutput {
    if (!util.isSupportedProfileVersionId(input.profile_version_id)) return types.ControlPlaneError.InvalidIdShape;

    var q = try conn.query(
        \\SELECT v.profile_id, v.is_valid, p.tenant_id
        \\FROM agent_profile_versions v
        \\JOIN agent_profiles p ON p.profile_id = v.profile_id
        \\WHERE p.workspace_id = $1 AND v.profile_version_id = $2
        \\LIMIT 1
    , .{ workspace_id, input.profile_version_id });
    defer q.deinit();

    const row = (try q.next()) orelse return types.ControlPlaneError.ProfileNotFound;
    const profile_id = try row.get([]const u8, 0);
    const is_valid = try row.get(bool, 1);
    const tenant_id = try row.get([]const u8, 2);
    if (!is_valid) return types.ControlPlaneError.ProfileInvalid;

    const now_ms = std.time.milliTimestamp();
    const activated_by = input.activated_by orelse "api";

    try beginTx(conn);
    var tx_open = true;
    errdefer if (tx_open) rollbackTx(conn);

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
    try profile_linkage.insertActivateArtifact(conn, tenant_id, workspace_id, input.profile_version_id, activated_by, now_ms);
    try commitTx(conn);
    tx_open = false;

    return .{
        .profile_id = profile_id,
        .profile_version_id = input.profile_version_id,
        .run_snapshot_version = input.profile_version_id,
        .activated_by = activated_by,
        .activated_at = now_ms,
    };
}
