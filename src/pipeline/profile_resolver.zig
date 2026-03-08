const std = @import("std");
const pg = @import("pg");
const topology = @import("topology.zig");

pub fn loadWorkspaceActiveProfile(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
) !?topology.Profile {
    var q = try conn.query(
        \\SELECT v.compiled_profile_json
        \\FROM workspace_active_profile wap
        \\JOIN agent_profile_versions v ON v.profile_version_id = wap.profile_version_id
        \\WHERE wap.workspace_id = $1 AND v.is_valid = TRUE
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();

    const row = try q.next() orelse return null;
    const compiled_opt = try row.get(?[]const u8, 0);
    const compiled = compiled_opt orelse return null;
    return topology.parseProfileJson(alloc, compiled);
}
