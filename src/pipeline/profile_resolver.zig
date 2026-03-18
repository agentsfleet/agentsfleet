const std = @import("std");
const pg = @import("pg");
const topology = @import("topology.zig");

pub fn loadWorkspaceActiveProfile(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
) !?topology.Profile {
    {
        var tenant_q = conn.query(
            "SELECT tenant_id FROM workspaces WHERE workspace_id = $1 LIMIT 1",
            .{workspace_id},
        ) catch null;
        if (tenant_q) |*tq| {
            defer tq.*.deinit();
            if (tq.*.next() catch null) |tenant_row| {
                const tenant_id = tenant_row.get([]const u8, 0) catch null;
                if (tenant_id) |tid| {
                    _ = conn.exec(
                        "SELECT set_config('app.current_tenant_id', $1, true)",
                        .{tid},
                    ) catch {};
                }
            }
            tq.*.drain() catch {};
        }
    }

    var q = try conn.query(
        \\SELECT v.compiled_profile_json
        \\FROM workspace_active_config wap
        \\JOIN agent_config_versions v ON v.config_version_id = wap.config_version_id
        \\WHERE wap.workspace_id = $1 AND v.is_valid = TRUE
        \\LIMIT 1
    , .{workspace_id});
    defer q.deinit();

    const row = try q.next() orelse return null;
    const compiled_opt = try row.get(?[]const u8, 0);
    const compiled = compiled_opt orelse return null;
    const profile = try topology.parseProfileJson(alloc, compiled);
    try q.drain();
    return profile;
}
