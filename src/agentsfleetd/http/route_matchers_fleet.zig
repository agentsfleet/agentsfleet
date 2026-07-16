//! Fleet operator-plane route matchers.

const S_EVENTS = "events";
const S_FLEETS = "fleets";
const S_MEMORIES = "memories";
const S_RUNNERS = "runners";
const S_WORKSPACES = "workspaces";

pub const WorkspaceFleetMemoryRoute = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    memory_key: []const u8,
};

/// Match `/workspaces/{workspace_id}/fleets/{fleet_id}/memories/{key}`.
pub fn matchWorkspaceFleetMemoryItem(p: anytype) ?WorkspaceFleetMemoryRoute {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEETS) or !p.eq(4, S_MEMORIES)) return null;
    return .{
        .workspace_id = p.param(1) orelse return null,
        .fleet_id = p.param(3) orelse return null,
        .memory_key = p.param(5) orelse return null,
    };
}

/// Match `/fleets/runners/{runner_id}` after the `/v1` prefix is stripped.
pub fn matchFleetRunner(p: anytype) ?[]const u8 {
    if (p.segs.len != 3) return null;
    if (!p.eq(0, S_FLEETS) or !p.eq(1, S_RUNNERS)) return null;
    return p.param(2);
}

/// Match `/fleets/runners/{runner_id}/events` after the `/v1` prefix is stripped.
pub fn matchFleetRunnerEvents(p: anytype) ?[]const u8 {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_FLEETS) or !p.eq(1, S_RUNNERS) or !p.eq(3, S_EVENTS)) return null;
    return p.param(2);
}
