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
