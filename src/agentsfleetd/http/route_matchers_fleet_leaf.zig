// Per-Fleet sub-resource leaves — `/workspaces/{ws}/fleets/{fleet_id}/{leaf}/{id}`.
//
// Extracted from route_matchers.zig to keep it under the RULE FLL line cap.
// Every route here shares one shape (six segments, a reserved leaf literal at
// index 4, an id at 5) and each gets its own typed struct with a semantically
// named leaf field. Re-exported from route_matchers.zig so `matchers.match*`
// call sites stay unchanged.

const matchers = @import("route_matchers.zig");
const Path = matchers.Path;

const S_WORKSPACES = "workspaces";
const S_FLEETS = "fleets";
const S_BUNDLES = "bundles";
/// The `/events` sub-resource segment. Spelled once because two matchers share
/// this shape — `events/stream` (in route_matchers.zig) and `events/{event_id}`
/// here are both six segments, and a typo in either would silently make one
/// unroutable.
const S_EVENTS = "events";

/// A `fleets` segment that is not the start of `fleets/bundles`, which is a
/// different resource family entirely.
fn isFleetRuntimeSegment(p: Path, idx: usize) bool {
    return p.eq(idx, S_FLEETS) and (idx + 1 >= p.segs.len or !p.eq(idx + 1, S_BUNDLES));
}

const FleetLeafView = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    leaf: []const u8,
};

fn matchFleetLeaf(p: Path, leaf_segment: []const u8) ?FleetLeafView {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, S_WORKSPACES) or !isFleetRuntimeSegment(p, 2)) return null;
    if (!p.eq(4, leaf_segment)) return null;
    const ws = p.param(1) orelse return null;
    const fleet_id = p.param(3) orelse return null;
    const leaf = p.param(5) orelse return null;
    return .{ .workspace_id = ws, .fleet_id = fleet_id, .leaf = leaf };
}

pub const WorkspaceFleetGrantRoute = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    grant_id: []const u8,
};

pub fn matchWorkspaceFleetGrant(p: Path) ?WorkspaceFleetGrantRoute {
    const v = matchFleetLeaf(p, "integration-grants") orelse return null;
    return .{ .workspace_id = v.workspace_id, .fleet_id = v.fleet_id, .grant_id = v.leaf };
}

pub const WorkspaceFleetEventRoute = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
};

/// `/events/{event_id}` shares its six-segment shape with `/events/stream`.
/// `event_id` is TEXT on the table — it arrives from the producer, so there is
/// no id shape that would exclude the literal `stream`. The router resolves the
/// ambiguity by order (deepest, most literal shape first), which is why
/// `matchWorkspaceFleetEventsStream` is tried before this.
pub fn matchWorkspaceFleetEvent(p: Path) ?WorkspaceFleetEventRoute {
    const v = matchFleetLeaf(p, S_EVENTS) orelse return null;
    return .{ .workspace_id = v.workspace_id, .fleet_id = v.fleet_id, .event_id = v.leaf };
}
