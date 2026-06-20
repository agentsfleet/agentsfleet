//! Fleet Bundle route matchers.

const S_WORKSPACES = "workspaces";
const S_FLEETS = "fleets";
const S_BUNDLES = "bundles";
const S_SNAPSHOTS = "snapshots";

pub const WorkspaceFleetBundleRoute = struct {
    workspace_id: []const u8,
    bundle_id: []const u8,
};

/// Match `/workspaces/{workspace_id}/fleets/bundles/snapshots`.
pub fn matchWorkspaceFleetBundles(p: anytype) ?[]const u8 {
    if (p.segs.len != 5) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEETS) or !p.eq(3, S_BUNDLES) or !p.eq(4, S_SNAPSHOTS)) return null;
    return p.param(1);
}

/// Match `/workspaces/{workspace_id}/fleets/bundles/snapshots/{bundle_id}`.
pub fn matchWorkspaceFleetBundle(p: anytype) ?WorkspaceFleetBundleRoute {
    if (p.segs.len != 6) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEETS) or !p.eq(3, S_BUNDLES) or !p.eq(4, S_SNAPSHOTS)) return null;
    const workspace_id = p.param(1) orelse return null;
    const bundle_id = p.param(5) orelse return null;
    return .{ .workspace_id = workspace_id, .bundle_id = bundle_id };
}
