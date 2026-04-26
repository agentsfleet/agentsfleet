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
    return matchWorkspaceZombieSuffix(path, "/telemetry");
}

// M24_001: generic helper for /v1/workspaces/{ws}/zombies/{id}/{suffix} routes.
// Returns ZombieTelemetryRoute (ws_id + zombie_id) for any suffix.
pub fn matchWorkspaceZombieSuffix(path: []const u8, suffix: []const u8) ?ZombieTelemetryRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/zombies/";

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

// matchWebhookAction matches /v1/webhooks/{zombie_id}{action} and returns the zombie_id.
// `action` is the full suffix (e.g. "/approval") — M28 migration replaced the
// Google-style ":action" custom-method form with a direct subpath so public docs
// can parameterize it as /v1/webhooks/{zombie_id}/{action} without OpenAPI-validator
// rejection of the colon.
pub fn matchWebhookAction(path: []const u8, action: []const u8) ?[]const u8 {
    const prefix = "/v1/webhooks/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, action)) return null;
    const inner = path[prefix.len .. path.len - action.len];
    if (!isSingleSegment(inner)) return null;
    return inner;
}

// matchWorkspaceZombieAction matches /v1/workspaces/{ws}/zombies/{zombie_id}{action}.
// `action` is the full suffix (e.g. "/steer") — M28 migration replaced
// the Google-style ":action" custom-method form with a validator-friendly subpath.
pub fn matchWorkspaceZombieAction(path: []const u8, action: []const u8) ?WorkspaceZombieRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/zombies/";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    if (!std.mem.endsWith(u8, path, action)) return null;

    const inner = path[prefix.len .. path.len - action.len];
    const sep = std.mem.indexOf(u8, inner, mid) orelse return null;
    const ws_id = inner[0..sep];
    const zombie_id = inner[sep + mid.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (!isSingleSegment(zombie_id)) return null;
    return .{ .workspace_id = ws_id, .zombie_id = zombie_id };
}

// M24_001: WorkspaceZombieRoute carries workspace_id + zombie_id for /v1/workspaces/{ws}/zombies/{zombie_id}.
pub const WorkspaceZombieRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
};

// M24_001: matchWorkspaceZombie matches /v1/workspaces/{ws_id}/zombies/{zombie_id}.
// Used for DELETE and (in later slices) per-zombie sub-resources.
pub fn matchWorkspaceZombie(path: []const u8) ?WorkspaceZombieRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/zombies/";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const sep = std.mem.indexOf(u8, rest, mid) orelse return null;
    const ws_id = rest[0..sep];
    const zombie_id = rest[sep + mid.len ..];

    if (!isSingleSegment(ws_id)) return null;
    if (!isSingleSegment(zombie_id)) return null;
    return .{ .workspace_id = ws_id, .zombie_id = zombie_id };
}

// WorkspaceCredentialRoute carries workspace_id + credential_name for the
// per-credential DELETE endpoint.
pub const WorkspaceCredentialRoute = struct {
    workspace_id: []const u8,
    credential_name: []const u8,
};

// matchWorkspaceCredential matches /v1/workspaces/{ws}/credentials/{name}.
// Rejects /credentials/llm — that suffix is owned by the BYOK route family.
pub fn matchWorkspaceCredential(path: []const u8) ?WorkspaceCredentialRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/credentials/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const slash = std.mem.indexOf(u8, rest, mid) orelse return null;
    const workspace_id = rest[0..slash];
    if (!isSingleSegment(workspace_id)) return null;
    const credential_name = rest[slash + mid.len ..];
    if (!isSingleSegment(credential_name)) return null;
    if (std.mem.eql(u8, credential_name, "llm")) return null;
    return .{ .workspace_id = workspace_id, .credential_name = credential_name };
}

// M9_001 / M28_002 §0: WorkspaceAgentRoute carries workspace_id + agent_id for agent-key DELETE.
pub const WorkspaceAgentRoute = struct {
    workspace_id: []const u8,
    agent_id: []const u8,
};

// M9_001 / M28_002 §0: matchWorkspaceAgentDelete matches /v1/workspaces/{ws}/agent-keys/{agent_id}.
pub fn matchWorkspaceAgentDelete(path: []const u8) ?WorkspaceAgentRoute {
    const prefix = "/v1/workspaces/";
    const mid = "/agent-keys/";
    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];
    const slash = std.mem.indexOf(u8, rest, mid) orelse return null;
    const workspace_id = rest[0..slash];
    if (!isSingleSegment(workspace_id)) return null;
    const agent_id = rest[slash + mid.len ..];
    if (!isSingleSegment(agent_id)) return null;
    return .{ .workspace_id = workspace_id, .agent_id = agent_id };
}

// M24_001: WorkspaceZombieGrantRoute carries ws_id + zombie_id + grant_id for grant DELETE.
pub const WorkspaceZombieGrantRoute = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
    grant_id: []const u8,
};

// M24_001: matchWorkspaceZombieGrant matches
//   /v1/workspaces/{ws}/zombies/{zombie_id}/integration-grants/{grant_id}.
pub fn matchWorkspaceZombieGrant(path: []const u8) ?WorkspaceZombieGrantRoute {
    const prefix = "/v1/workspaces/";
    const ws_mid = "/zombies/";
    const grant_mid = "/integration-grants/";

    if (!std.mem.startsWith(u8, path, prefix)) return null;
    const rest = path[prefix.len..];

    const ws_sep = std.mem.indexOf(u8, rest, ws_mid) orelse return null;
    const ws_id = rest[0..ws_sep];
    if (!isSingleSegment(ws_id)) return null;

    const after_ws = rest[ws_sep + ws_mid.len ..];
    const grant_sep = std.mem.indexOf(u8, after_ws, grant_mid) orelse return null;
    const zombie_id = after_ws[0..grant_sep];
    if (!isSingleSegment(zombie_id)) return null;

    const grant_id = after_ws[grant_sep + grant_mid.len ..];
    if (!isSingleSegment(grant_id)) return null;

    return .{ .workspace_id = ws_id, .zombie_id = zombie_id, .grant_id = grant_id };
}

test "matchWorkspaceCredential: workspace_id and credential_name" {
    const r = matchWorkspaceCredential("/v1/workspaces/ws1/credentials/fly").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("fly", r.credential_name);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces/ws1/credentials/") == null);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces//credentials/fly") == null);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces/ws1/credentials/llm") == null);
    try std.testing.expect(matchWorkspaceCredential("/v1/workspaces/ws1/credentials") == null);
}

test "matchWorkspaceAgentDelete: workspace_id and agent_id" {
    const r = matchWorkspaceAgentDelete("/v1/workspaces/ws1/agent-keys/ag1").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("ag1", r.agent_id);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces/ws1/agent-keys/") == null);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces//agent-keys/ag1") == null);
    try std.testing.expect(matchWorkspaceAgentDelete("/v1/workspaces/a/b/agent-keys/ag1") == null);
}

test "matchWorkspaceZombieGrant: ws_id, zombie_id, grant_id" {
    const r = matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies/z1/integration-grants/g1").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("z1", r.zombie_id);
    try std.testing.expectEqualStrings("g1", r.grant_id);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies/z1/integration-grants/") == null);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces//zombies/z1/integration-grants/g1") == null);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies//integration-grants/g1") == null);
    try std.testing.expect(matchWorkspaceZombieGrant("/v1/workspaces/ws1/zombies/z1/x/integration-grants/g1") == null);
}

test "matchWorkspaceZombie: workspace_id and zombie_id extracted" {
    const r = matchWorkspaceZombie("/v1/workspaces/ws_1/zombies/z_1").?;
    try std.testing.expectEqualStrings("ws_1", r.workspace_id);
    try std.testing.expectEqualStrings("z_1", r.zombie_id);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces/ws_1/zombies/") == null);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces//zombies/z_1") == null);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces/a/b/zombies/z_1") == null);
    try std.testing.expect(matchWorkspaceZombie("/v1/workspaces/ws_1/zombies/z_1/extra") == null);
}

test "matchWorkspaceZombieAction: /steer extracts ws_id + zombie_id" {
    const r = matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies/z1/steer", "/steer").?;
    try std.testing.expectEqualStrings("ws1", r.workspace_id);
    try std.testing.expectEqualStrings("z1", r.zombie_id);
    // empty ids rejected
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies//steer", "/steer") == null);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces//zombies/z1/steer", "/steer") == null);
    // multi-segment rejected
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies/a/b/steer", "/steer") == null);
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/a/b/zombies/z1/steer", "/steer") == null);
    // wrong action rejected
    try std.testing.expect(matchWorkspaceZombieAction("/v1/workspaces/ws1/zombies/z1/other-action", "/steer") == null);
    // flat path no longer matches
    try std.testing.expect(matchWorkspaceZombieAction("/v1/zombies/z1/steer", "/steer") == null);
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
