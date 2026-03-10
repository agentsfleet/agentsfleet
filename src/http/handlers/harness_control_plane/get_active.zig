const std = @import("std");
const pg = @import("pg");
const topology = @import("../../../pipeline/topology.zig");
const harness = @import("../../../harness/control_plane.zig");
const types = @import("types.zig");

pub fn getActiveProfile(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) (types.ControlPlaneError || anyerror)!types.ActiveOutput {
    var q = try conn.query(
        \\SELECT wap.profile_version_id, wap.activated_at, v.profile_id, v.compiled_profile_json
        \\FROM workspace_active_profile wap
        \\JOIN agent_profile_versions v ON v.profile_version_id = wap.profile_version_id
        \\WHERE wap.workspace_id = $1
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();

    if (try q.next()) |row| {
        const profile_version_id = try row.get([]const u8, 0);
        const activated_at = try row.get(i64, 1);
        const profile_id = try row.get([]const u8, 2);
        const compiled_json_opt = try row.get(?[]const u8, 3);
        if (compiled_json_opt) |compiled_json| {
            return .{
                .source = "active",
                .profile_id = profile_id,
                .profile_version_id = profile_version_id,
                .run_snapshot_version = profile_version_id,
                .active_at = activated_at,
                .profile_json = try alloc.dupe(u8, compiled_json),
            };
        }
    }

    var fallback = try topology.defaultProfile(alloc);
    defer fallback.deinit();
    const fallback_json = try harness.stringifyTopologyProfile(alloc, &fallback);
    return .{
        .source = "default-v1",
        .profile_id = null,
        .profile_version_id = null,
        .run_snapshot_version = null,
        .active_at = null,
        .profile_json = fallback_json,
    };
}
