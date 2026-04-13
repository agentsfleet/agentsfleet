// Route matching helpers for the HTTP router.
//
// Extracted from router.zig to keep files under 400 lines.
// Pure functions that parse URL paths into route parameters.

const std = @import("std");
const router = @import("router.zig");

pub const WebhookRoute = router.WebhookRoute;
pub const ZombieTelemetryRoute = router.ZombieTelemetryRoute;

const prefix_workspaces = "/v1/workspaces/";
const prefix_agents = "/v1/agents/";

// M10_001: matchRunAction removed — /v1/runs/* routes deleted.

pub fn matchWorkspaceSuffix(path: []const u8, suffix: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, path, prefix_workspaces)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_workspaces.len .. path.len - suffix.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

pub fn isSingleSegment(value: []const u8) bool {
    return value.len > 0 and std.mem.indexOfScalar(u8, value, '/') == null;
}

// matchZombieTelemetry matches /v1/workspaces/{ws_id}/zombies/{zombie_id}/telemetry.
pub fn matchZombieTelemetry(path: []const u8) ?ZombieTelemetryRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/zombies/";
    const suffix = "/telemetry";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;

    const inner = path[prefix.len .. path.len - suffix.len];
    const sep = std.mem.indexOf(u8, inner, mid) orelse return null;
    const ws_id = inner[0..sep];
    const zombie_id = inner[sep + mid.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (!isSingleSegment(zombie_id)) return null;
    return .{ .workspace_id = ws_id, .zombie_id = zombie_id };
}

// matchWebhookRoute matches /v1/webhooks/{zombie_id} or /v1/webhooks/{zombie_id}/{secret}.
pub fn matchWebhookRoute(path: []const u8) ?WebhookRoute {
    const prefix = "/v1/webhooks/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    if (rest.len == 0) return null;

    if (std.mem.indexOfScalar(u8, rest, '/')) |slash| {
        const zombie_id = rest[0..slash];
        const secret = rest[slash + 1 ..];
        if (zombie_id.len == 0 or secret.len == 0) return null;
        if (std.mem.indexOfScalar(u8, secret, '/') != null) return null;
        return .{ .zombie_id = zombie_id, .secret = secret };
    }

    return .{ .zombie_id = rest, .secret = null };
}

// ── Tests ──────────────────────────────────────────────────────────────

// M10_001: matchRunAction test removed — function deleted.

// M4_001: matchWebhookAction matches /v1/webhooks/{zombie_id}{action} and returns the zombie_id.
pub fn matchWebhookAction(path: []const u8, action: []const u8) ?[]const u8 {
    const prefix = "/v1/webhooks/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, action)) return null;
    const inner = path[prefix.len .. path.len - action.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

// M2_001: matchZombieId matches /v1/zombies/{zombie_id} for DELETE.
pub fn matchZombieId(path: []const u8) ?[]const u8 {
    const prefix = "/v1/zombies/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const zombie_id = path[prefix.len..];
    if (std.mem.eql(u8, zombie_id, "activity") or std.mem.eql(u8, zombie_id, "credentials")) return null;
    if (!isSingleSegment(zombie_id)) return null;
    return zombie_id;
}

// M9_001: WorkspaceAgentRoute carries workspace_id + agent_id for external-agent DELETE.
pub const WorkspaceAgentRoute = struct {
    workspace_id: []const u8,
    agent_id: []const u8,
};

// M9_001: matchWorkspaceAgentDelete matches /v1/workspaces/{ws}/external-agents/{agent_id}.
pub fn matchWorkspaceAgentDelete(path: []const u8) ?WorkspaceAgentRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/external-agents/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const slash = std.mem.indexOf(u8, rest, mid) orelse return null;
    const workspace_id = rest[0..slash];
    if (!isSingleSegment(workspace_id)) return null;
    const agent_id = rest[slash + mid.len ..];
    if (!isSingleSegment(agent_id)) return null;
    return .{ .workspace_id = workspace_id, .agent_id = agent_id };
}

// M9_001: matchZombieSuffix matches /v1/zombies/{id}/{suffix} and returns the zombie_id.
pub fn matchZombieSuffix(path: []const u8, suffix: []const u8) ?[]const u8 {
    const prefix = "/v1/zombies/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix.len .. path.len - suffix.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

// M9_001: ZombieGrantRoute carries zombie_id + grant_id for DELETE grant.
pub const ZombieGrantRoute = struct {
    zombie_id: []const u8,
    grant_id: []const u8,
};

// M9_001: matchZombieGrantRevoke matches /v1/zombies/{zombie_id}/integration-grants/{grant_id}.
pub fn matchZombieGrantRevoke(path: []const u8) ?ZombieGrantRoute {
    const prefix = "/v1/zombies/";
    const mid = "/integration-grants/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const slash = std.mem.indexOf(u8, rest, mid) orelse return null;
    const zombie_id = rest[0..slash];
    if (!isSingleSegment(zombie_id)) return null;
    const grant_id = rest[slash + mid.len ..];
    if (!isSingleSegment(grant_id)) return null;
    return .{ .zombie_id = zombie_id, .grant_id = grant_id };
}

test "matchWorkspaceAgentDelete: workspace_id and agent_id" {
    const r = matchWorkspaceAgentDelete("/v1/workspaces/ws1/external-agents/ag1").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("ag1", r.agent_id);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces/ws1/external-agents/") == null);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces//external-agents/ag1") == null);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces/a/b/external-agents/ag1") == null);
}

test "matchZombieSuffix: integration-requests and integration-grants" {
    const id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(id, matchZombieSuffix("/v1/zombies/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/integration-requests", "/integration-requests").?);
    try std.testing.expectEqualStrings(id, matchZombieSuffix("/v1/zombies/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11/integration-grants", "/integration-grants").?);
    try std.testing.expect(matchZombieSuffix("/v1/zombies//integration-requests", "/integration-requests") == null);
    try std.testing.expect(matchZombieSuffix("/v1/zombies/a/b/integration-requests", "/integration-requests") == null);
}

test "matchZombieGrantRevoke: zombie_id and grant_id" {
    const r = matchZombieGrantRevoke("/v1/zombies/z1/integration-grants/g1").?;
    try std.testing.expectEqualStrings("z1", r.zombie_id);
    try std.testing.expectEqualStrings("g1", r.grant_id);
    try std.testing.expect(matchZombieGrantRevoke("/v1/zombies/z1/integration-grants/") == null);
    try std.testing.expect(matchZombieGrantRevoke("/v1/zombies//integration-grants/g1") == null);
    try std.testing.expect(matchZombieGrantRevoke("/v1/zombies/z1/z2/integration-grants/g1") == null);
}

test "matchZombieId: excludes sub-paths" {
    try std.testing.expect(matchZombieId("/v1/zombies/activity") == null);
    try std.testing.expect(matchZombieId("/v1/zombies/credentials") == null);
    try std.testing.expectEqualStrings("z1", matchZombieId("/v1/zombies/z1").?);
    try std.testing.expect(matchZombieId("/v1/zombies/a/b") == null);
}

test "matchZombieTelemetry: extracts workspace_id and zombie_id" {
    const ws = "ws_abc";
    const zid = "z_123";
    const r = matchZombieTelemetry("/v1/workspaces/ws_abc/zombies/z_123/telemetry").?;
    try std.testing.expectEqualStrings(ws, r.workspace_id);
    try std.testing.expectEqualStrings(zid, r.zombie_id);
    // extra segments rejected
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces/ws_abc/extra/zombies/z_123/telemetry") == null);
    // missing trailing segment
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces/ws_abc/zombies/z_123") == null);
    // empty zombie_id
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces/ws_abc/zombies//telemetry") == null);
    // empty workspace_id
    try std.testing.expect(matchZombieTelemetry("/v1/workspaces//zombies/z_123/telemetry") == null);
}

test "matchWebhookRoute: id only and id+secret" {
    const id = "019abc12-8d3a-7f13-8abc-2b3e1e0a6f11";
    const r1 = matchWebhookRoute("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11").?;
    try std.testing.expectEqualStrings(id, r1.zombie_id);
    try std.testing.expect(r1.secret == null);
    const r2 = matchWebhookRoute("/v1/webhooks/019abc12-8d3a-7f13-8abc-2b3e1e0a6f11/kR7x2mN").?;
    try std.testing.expectEqualStrings(id, r2.zombie_id);
    try std.testing.expectEqualStrings("kR7x2mN", r2.secret.?);
    try std.testing.expect(matchWebhookRoute("/v1/webhooks/") == null);
    try std.testing.expect(matchWebhookRoute("/v1/webhooks") == null);
    try std.testing.expect(matchWebhookRoute("/v1/webhooks/a/b/c") == null);
}
