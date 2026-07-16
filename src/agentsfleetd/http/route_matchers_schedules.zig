//! Schedule route matchers split from route_matchers.zig for the line cap.

const std = @import("std");

const S_WORKSPACES = "workspaces";
const S_FLEETS = "fleets";
const S_SCHEDULES = "schedules";
const OP_SYNC = ":sync";

pub const WorkspaceFleetScheduleCollectionRoute = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
};

pub const WorkspaceFleetScheduleRoute = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    schedule_id: []const u8,
};

pub fn matchScheduleCollection(p: anytype) ?WorkspaceFleetScheduleCollectionRoute {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEETS) or !p.eq(4, S_SCHEDULES)) return null;
    return .{ .workspace_id = p.param(1) orelse return null, .fleet_id = p.param(3) orelse return null };
}

pub fn matchScheduleItem(p: anytype) ?WorkspaceFleetScheduleRoute {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEETS) or !p.eq(4, S_SCHEDULES)) return null;
    return scheduleRoute(p);
}

pub fn matchScheduleSync(p: anytype) ?WorkspaceFleetScheduleRoute {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEETS) or !p.eq(4, S_SCHEDULES)) return null;
    const leaf = p.param(5) orelse return null;
    if (!std.mem.endsWith(u8, leaf, OP_SYNC) or leaf.len <= OP_SYNC.len) return null;
    const schedule_id = leaf[0 .. leaf.len - OP_SYNC.len];
    if (std.mem.indexOfScalar(u8, schedule_id, ':') != null) return null;
    return .{
        .workspace_id = p.param(1) orelse return null,
        .fleet_id = p.param(3) orelse return null,
        .schedule_id = schedule_id,
    };
}

fn scheduleRoute(p: anytype) ?WorkspaceFleetScheduleRoute {
    return .{
        .workspace_id = p.param(1) orelse return null,
        .fleet_id = p.param(3) orelse return null,
        .schedule_id = p.param(5) orelse return null,
    };
}
