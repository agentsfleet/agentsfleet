// Route matching helpers for the HTTP router.
//
// Extracted from router.zig to keep files under 400 lines.
// Pure functions that parse URL paths into route parameters.

const std = @import("std");
const router = @import("router.zig");

const AgentProposalRoute = router.AgentProposalRoute;
const AgentHarnessChangeRoute = router.AgentHarnessChangeRoute;
pub const WebhookRoute = router.WebhookRoute;

const prefix_workspaces = "/v1/workspaces/";
const prefix_runs = "/v1/runs/";
const prefix_agents = "/v1/agents/";

/// M16_002: Match /v1/runs/<run_id><action> routes generically.
pub fn matchRunAction(path: []const u8, action: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, path, prefix_runs)) return null;
    if (!std.mem.endsWith(u8, path, action)) return null;
    const inner = path[prefix_runs.len .. path.len - action.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

pub fn matchWorkspaceSuffix(path: []const u8, suffix: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, path, prefix_workspaces)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_workspaces.len .. path.len - suffix.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

pub fn matchAgentProposalAction(path: []const u8, suffix: []const u8) ?AgentProposalRoute {
    if (!std.mem.startsWith(u8, path, prefix_agents)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_agents.len .. path.len - suffix.len];
    const marker = "/proposals/";
    const marker_idx = std.mem.indexOf(u8, inner, marker) orelse return null;
    const agent_id = inner[0..marker_idx];
    const proposal_id = inner[marker_idx + marker.len ..];
    if (!isSingleSegment(agent_id) or !isSingleSegment(proposal_id)) return null;
    return .{ .agent_id = agent_id, .proposal_id = proposal_id };
}

pub fn matchAgentHarnessChangeAction(path: []const u8, suffix: []const u8) ?AgentHarnessChangeRoute {
    if (!std.mem.startsWith(u8, path, prefix_agents)) return null;
    if (!std.mem.endsWith(u8, path, suffix)) return null;
    const inner = path[prefix_agents.len .. path.len - suffix.len];
    const marker = "/harness/changes/";
    const marker_idx = std.mem.indexOf(u8, inner, marker) orelse return null;
    const agent_id = inner[0..marker_idx];
    const change_id = inner[marker_idx + marker.len ..];
    if (!isSingleSegment(agent_id) or !isSingleSegment(change_id)) return null;
    return .{ .agent_id = agent_id, .change_id = change_id };
}

pub fn isSingleSegment(value: []const u8) bool {
    return value.len > 0 and std.mem.indexOfScalar(u8, value, '/') == null;
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

test "matchRunAction resolves actions with single-segment run_id" {
    const run_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:retry", ":retry").?);
    try std.testing.expectEqualStrings(run_id, matchRunAction("/v1/runs/0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11:replay", ":replay").?);
    try std.testing.expect(matchRunAction("/v1/runs/foo/bar:retry", ":retry") == null);
    try std.testing.expect(matchRunAction("/v1/runs/:retry", ":retry") == null);
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
