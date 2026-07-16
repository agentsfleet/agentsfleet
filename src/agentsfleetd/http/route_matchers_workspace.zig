// Workspace item-leaf matchers — `/workspaces/{workspace_id}/{collection}/{leaf}`.
//
// Extracted from route_matchers.zig to keep it under the RULE FLL line cap.
// These two share one shape (four segments, a reserved collection literal at
// index 2, a workspace id at 1, and a leaf id at 3) and are re-exported from
// route_matchers.zig so `matchers.matchWorkspace*` call sites stay unchanged.

const matchers = @import("route_matchers.zig");
const Path = matchers.Path;

const S_WORKSPACES = "workspaces";
const S_SECRETS = "secrets";
const S_FLEET_KEYS = "fleet-keys";

// ── /workspaces/{ws}/secrets/{name} ────────────────────────────────────────

pub const WorkspaceSecretRoute = struct {
    workspace_id: []const u8,
    secret_name: []const u8,
};

pub fn matchWorkspaceSecret(p: Path) ?WorkspaceSecretRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_SECRETS)) return null;
    const ws = p.param(1) orelse return null;
    const name = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .secret_name = name };
}

// ── /workspaces/{ws}/preferences/{pref_key} ────────────────────────────────

pub const WorkspacePreferenceRoute = struct {
    workspace_id: []const u8,
    pref_key: []const u8,
};

pub fn matchWorkspacePreference(p: Path) ?WorkspacePreferenceRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, matchers.S_PREFERENCES)) return null;
    const ws = p.param(1) orelse return null;
    const key = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .pref_key = key };
}

// ── /workspaces/{ws}/fleet-keys/{fleet_key_id} ───────────────────────────

pub const WorkspaceFleetKeyRoute = struct {
    workspace_id: []const u8,
    fleet_key_id: []const u8,
};

pub fn matchWorkspaceFleetKeyDelete(p: Path) ?WorkspaceFleetKeyRoute {
    if (p.segs.len != 4) return null;
    if (!p.eq(0, S_WORKSPACES) or !p.eq(2, S_FLEET_KEYS)) return null;
    const ws = p.param(1) orelse return null;
    const fleet_key_id = p.param(3) orelse return null;
    return .{ .workspace_id = ws, .fleet_key_id = fleet_key_id };
}
